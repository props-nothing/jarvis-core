# ADR-0199: A long run also leaves out the text it wrote in old calls

Status: Accepted
Date: 2026-10-11

## Context

`ADR-0194` keeps a long run's working context small by leaving out old, large tool **results**. It recorded one limit: "Large tool call arguments (a file written in one call) are not shortened." The owner's own runs show the limit matters. In the 24 hours before this change JARVIS recorded 272 runs and 115 million input tokens (91% reported as cached). One research sub-agent made 219 model calls and its request grew to 176,535 tokens; several others reached 100,000 to 109,000. These runs read little and write a lot (they save findings with `jarvis.files.write_document`), so their results are two words each and the bulk of the transcript is the text the model itself wrote, which the trim never touched. The 200 KB budget was a bound only for runs that read.

## Decision

1. **A second pass.** When leaving out old results has not brought the transcript to the low-water mark, long strings (600 bytes or more, at any depth) inside the **arguments of old calls** are replaced by `[left out of the working context: N characters, already sent]`. The arguments stay valid JSON with every other field, and the call keeps its name, id and place.
2. **Which calls.** Only calls whose results the model has already read and that are not among the newest six results' calls. A call in the newest turn is never changed, whatever it holds, so a call that is waiting for an approval is intact when it resumes. Arguments that are not JSON are left alone.
3. **Results go first.** The pass runs only when results were not enough, so a run that reads (the case `ADR-0194` measured) behaves exactly as before. The `context_trimmed` activity event now says how many calls had text left out (`arguments`).
4. **No loss of record.** The full call is in the run's stored events and the audit entry, and the file the model wrote is on disk; only the model's copy of it in later requests is shortened.

## Measured

- Unit tests: a transcript made of writes comes down to the low-water mark with every pair intact and the newest calls whole; the same transcript trimmed twice is changed once; results are used first; unanswered calls and non-JSON arguments are never changed. Disabling the pass fails two of them.
- Live, 2026-10-11, owner's daemon, `glm-5.3-flash`: a run asked to write 26 files of 120 lines each (about 10 KB per file, written one call at a time or a few at once). It trimmed once (`arguments: 4`, 83,944 bytes), finished in 164 s with 14 model calls, its largest request was 51,183 input tokens, and **all 26 files were exactly right** (every one of 3,120 lines compared), including those written after the trim. No placeholder text reached any file. One sample: evidence, not proof.
- **Eval suite, 2026-10-11:** `evals/long-task.toml` (two cases: 26 files written with several calls per turn, and strictly one per turn; the last files are read back and the answer must not contain the placeholder). Run with `--repeat 3` on `glm-5.3-flash`: **6 of 6 attempts passed**, and every file of all six attempts was checked on disk: 156 files, 18,720 lines, all exact, none containing placeholder text. Each run wrote 63,000 to 69,000 tokens and made 7 to 28 model calls, trimmed once or twice (2 to 12 calls shortened) and its largest request was **51,297 to 51,553 tokens** whether it made 7 calls or 28. Two lessons from building the suite: a first version reused one folder, and a model found the files already there and "passed" in 22 seconds with one write (so every attempt now uses a new folder named from the clock, and the report's seconds and tokens must be read too); and one attempt chose to delegate the work to four sub-agents (the prompt now forbids it). Still one model and one provider: the placeholder risk is measured at 0 of 6, not excluded for other models.

## Consequences

- The risk this adds is a model that reads `[left out ...]` in its own earlier call and copies the pattern into a new write. The wording says the text was sent, and `evals/long-task.toml` measured it at 0 of 6 attempts on one model; other models are not measured.
- Not done: the assistant's own prose between calls, and old `tool_discovery` bookkeeping, are not shortened; neither dominated the measured runs. The 200 KB budget itself is still a guess, now with one more live data point.
