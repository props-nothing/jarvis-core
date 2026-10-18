"""GPU selection fixtures run without torch, model assets, or GPU hardware."""
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest

spec = importlib.util.spec_from_file_location('voice_worker', Path(__file__).resolve().parents[1] / 'crates/jarvis-voice/src/description_worker.py')
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)


class Tensor:
    def __matmul__(self, other): return self
    def sum(self): return self
    def item(self): return 32768


class Runtime:
    def __init__(self, backend='amd', available=True, broken=()):
        self.version = SimpleNamespace(hip='7.2' if backend == 'amd' else None, cuda='12.8' if backend == 'nvidia' else None)
        self.available, self.broken, self.selected = available, broken, None
        self.cuda = self
        self.names = ['Integrated GPU', 'Discrete GPU']
    def is_available(self): return self.available
    def device_count(self): return 2
    def get_device_properties(self, index):
        return SimpleNamespace(total_memory=(2 if index == 0 else 16) * 1024**3, name=self.names[index])
    def ones(self, shape, device):
        if int(device.split(':')[1]) in self.broken: raise RuntimeError('unsupported kernel or driver')
        return Tensor()
    def synchronize(self, index): pass
    def set_device(self, index): self.selected = index


class SelectionTests(unittest.TestCase):
    def test_amd_and_nvidia_select_discrete_gpu(self):
        for backend in ['amd', 'nvidia']:
            runtime = Runtime(backend)
            self.assertEqual(worker.select_device(runtime, 'gpu'), ('cuda:1', backend, 'Discrete GPU'))
            self.assertEqual(runtime.selected, 1)

    def test_failed_adapter_tries_other_compatible_device(self):
        self.assertEqual(worker.select_device(Runtime(broken=(1,)), 'gpu')[0], 'cuda:0')

    def test_missing_gpu_bad_drivers_and_unknown_backend_fail_for_rust_cpu_retry(self):
        for runtime in [Runtime(available=False), Runtime(broken=(0, 1)), Runtime('unknown')]:
            with self.assertRaises(ValueError): worker.select_device(runtime, 'gpu')

    def test_cpu_does_not_touch_gpu_runtime(self):
        self.assertEqual(worker.select_device(None, 'cpu'), ('cpu', 'cpu', 'CPU'))


if __name__ == '__main__': unittest.main()
