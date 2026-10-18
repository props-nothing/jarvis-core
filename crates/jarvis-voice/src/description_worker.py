"""Private, pipe-only local inference adapter. Not a server or standalone service."""
import base64
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import struct
import sys
import wave


def select_device(torch, mode):
    """Probe kernels, not just adapter names. HIP deliberately uses torch.cuda."""
    if mode == 'cpu':
        return 'cpu', 'cpu', 'CPU'
    if mode != 'gpu' or not torch.cuda.is_available():
        raise ValueError('no compatible GPU')
    backend = 'amd' if torch.version.hip else 'nvidia' if torch.version.cuda else None
    if not backend:
        raise ValueError('unsupported GPU runtime')
    devices = []
    for index in range(torch.cuda.device_count()):
        try:
            props = torch.cuda.get_device_properties(index)
            devices.append((props.total_memory, index, props.name))
        except Exception:
            continue
    for _, index, name in sorted(devices, reverse=True):
        try:
            device = 'cuda:' + str(index)
            probe = torch.ones((32, 32), device=device)
            assert (probe @ probe).sum().item() == 32768
            del probe
            torch.cuda.synchronize(index)
            torch.cuda.set_device(index)
            return device, backend, name
        except Exception:
            continue
    raise ValueError('no compatible GPU kernels')


def metadata(torch, device, backend, name):
    return dict(backend=backend, device=name,
                allocated_bytes=torch.cuda.memory_allocated(device) if backend != 'cpu' else 0,
                peak_bytes=torch.cuda.max_memory_allocated(device) if backend != 'cpu' else 0)


def frame(kind, data=b''):
    output.write(kind + struct.pack('<I', len(data)) + data)
    output.flush()


def phase(value):
    frame(b'P', value.encode('ascii'))


def main():
    model_dir = Path(sys.argv[1])
    manifest = json.loads(sys.argv[2])
    output.write(b'JVD3'); output.flush()
    phase('checking')
    for item in manifest['files']:
        path = model_dir / item['name']
        if path.is_symlink() or not path.is_file() or path.stat().st_size != item['size']:
            raise ValueError('invalid local asset')
        with path.open('rb') as source:
            if hashlib.file_digest(source, 'sha256').hexdigest() != item['sha256']:
                raise ValueError('invalid local asset')
    phase('loading')
    os.environ.update(HF_HUB_OFFLINE='1', TRANSFORMERS_OFFLINE='1',
                      HF_HUB_DISABLE_TELEMETRY='1', DO_NOT_TRACK='1', TOKENIZERS_PARALLELISM='false')
    from importlib.metadata import version
    for package, expected in [('voxcpm', '2.0.3'), ('transformers', '4.57.6')]:
        if version(package).split('+')[0] != expected:
            raise ValueError('incompatible local runtime')
    import torch
    mode = sys.argv[3] if len(sys.argv) > 3 else 'cpu'
    expected = ('2.9.1+rocm7.2.1', '2.10.0+cu128') if mode == 'gpu' else ('2.10.0+cpu',)
    if version('torch') not in expected:
        raise ValueError('incompatible local runtime')
    import numpy as np
    from voxcpm import VoxCPM
    torch.set_num_threads(8); torch.set_num_interop_threads(1)
    device, backend, name = select_device(torch, mode)
    model = VoxCPM.from_pretrained(str(model_dir), load_denoiser=False, optimize=False,
                                   device=device, local_files_only=True).tts_model
    def info():
        frame(b'M', json.dumps(metadata(torch, device, backend, name)).encode('utf8'))
    def pcm(rendered):
        audio = rendered.squeeze(0).cpu().numpy()
        if not np.isfinite(audio).all(): raise ValueError('unusable audio')
        return (np.clip(audio, -1, 1) * 32767).round().astype('<i2').tobytes()
    def wave_bytes(data):
        buffer = io.BytesIO()
        with wave.open(buffer, 'wb') as wav:
            wav.setnchannels(1); wav.setsampwidth(2); wav.setframerate(48000); wav.writeframes(data)
        return buffer.getvalue()
    info(); phase('warming')
    torch.manual_seed(42); np.random.seed(42)
    rendered, _, _ = model.generate_with_prompt_cache(target_text='Hello. I am ready to help.',
        prompt_cache=None, cfg_value=2.0, inference_timesteps=10, max_len=62, retry_badcase=False)
    warm_cache = model.build_prompt_cache(reference_wav_path=io.BytesIO(wave_bytes(pcm(rendered))))
    for rendered, _, _ in model.generate_with_prompt_cache_streaming(target_text='Hello. I am ready to help.',
        prompt_cache=warm_cache, cfg_value=2.0, inference_timesteps=10, max_len=62, retry_badcase=False):
        pcm(rendered)
    del warm_cache, rendered
    info(); frame(b'D')
    previous_reference, prompt_cache = None, None
    while True:
        line = sys.stdin.buffer.readline(2 * 1024 * 1024 + 1)
        if not line: return
        if len(line) > 2 * 1024 * 1024 or not line.endswith(b'\n'): raise ValueError('oversized request')
        try:
            request = json.loads(line)
            if set(request) != {'text', 'description', 'reference', 'seed', 'steps'}: raise ValueError('invalid request')
            text, description = request['text'], request['description']
            if not isinstance(text, str) or not text.strip() or len(text) > 1500: raise ValueError('invalid text')
            if request['steps'] not in (4, 6, 10): raise ValueError('invalid steps')
            if description:
                if len(description) > 500: raise ValueError('invalid description')
                text = '(' + description.replace('(', ' ').replace(')', ' ') + ')' + text
            torch.manual_seed(request['seed']); np.random.seed(request['seed'])
            if request['reference'] != previous_reference:
                prompt_cache = None
                if request['reference']:
                    phase('encoding')
                    data = base64.b64decode(request['reference'], validate=True)
                    if len(data) > 960044: raise ValueError('oversized reference')
                    prompt_cache = model.build_prompt_cache(reference_wav_path=io.BytesIO(data))
                previous_reference = request['reference']
            phase('generating')
            size = 0
            for rendered, _, _ in model.generate_with_prompt_cache_streaming(target_text=text,
                prompt_cache=prompt_cache, cfg_value=2.0, inference_timesteps=request['steps'],
                max_len=62 if description else 375, retry_badcase=False):
                data = pcm(rendered); size += len(data)
                if not 0 < len(data) <= 96000 or size > 96000 * (10 if description else 60):
                    raise ValueError('oversized audio')
                frame(b'A', data)
            if not size: raise ValueError('empty audio')
            info(); frame(b'D')
        except Exception:
            frame(b'E'); return


if __name__ == '__main__':
    output = sys.stdout.buffer
    with open(os.devnull, 'w') as quiet, contextlib.redirect_stdout(quiet), contextlib.redirect_stderr(quiet):
        try: main()
        except Exception: sys.exit(1)
