# Deleting and recovering files

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
