# Reading and recovering files

`jarvis.files.read` can read beyond the first page of a UTF-8 file, with no total line-count limit.
Its JSON result contains `content`, a byte `offset`, `truncated` and `next_offset`.
Continue with the returned `next_offset` until it is null. Reads remain confined to granted folders.
The file is read live; changing it between pages can change the result.

Long chat requests are retained up to 200,000 characters. The console uses a single-line **Message** input;
the native model can retrieve the rest of a long request through `jarvis.context.read` with `source=input`.
That tool can also discover older messages with `source=history` and read a message by its returned ID.
Only the current conversation is accessible. Pages use character offsets except history, which uses message sequences.

`jarvis.agent.result` returns `next_offset` when a delegated answer continues. Follow it rather than rerunning the task.

Command results expose `stdout_path` and `stderr_path` for captured output in `.jarvis-output` under a granted folder.
Use the file reader to inspect omitted output. `capture_complete=false` means the capture failed or is partial;
each stream retains at most 16 MiB. These files remain until removed and can contain sensitive workspace output.
Do not automatically commit them. Model windows, document extraction ceilings and third-party tool limits still apply.

## Deleting and recovering files

`jarvis.files.delete` takes one `path` relative to a folder granted in Settings, Folders & code.
Deletion asks for approval by default; the owner may explicitly grant standing trust in Settings, Tools.
It moves a **regular file** into `.jarvis-trash/<unique-id>/<original-name>`
in the same granted folder and returns `trash_path` and the owning `root`.

This is recoverable **workspace-local trash**, not the Windows Recycle Bin or another operating system's trash.
It never permanently deletes files, recursively removes folders or purges trash. Directories, links, paths outside
granted folders and deletion from trash are refused. Trashed content still takes disk space until the owner removes
it manually. File search skips trash; the knowledge index drops the old file at its next refresh and does not index trash.

To recover, ask JARVIS to move the returned `trash_path` back to its original relative `path` using
`jarvis.files.move`, or move it yourself. Restore never overwrites an existing file. With several granted folders,
use the returned root to identify the recovery location.

Gmail deletion is separate: see [Google tools](google.md). Contact deletion and CSV import are described in
[Contacts](contacts.md). Existing spreadsheet editing is described in [Spreadsheets](spreadsheets.md).
