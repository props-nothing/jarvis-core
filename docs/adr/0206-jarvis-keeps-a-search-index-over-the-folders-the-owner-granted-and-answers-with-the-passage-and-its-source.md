# ADR-0206: JARVIS keeps a search index over the folders the owner granted, and answers from it with the passage and its source

Status: Accepted
Date: 2026-10-13

## Context

JARVIS could read a file it was told the name of and search a folder for lines containing a word (`jarvis.files.search`), but it could not answer "what do my files say about the Acme contract" without opening files one by one: it did not know which file to open, and a PDF, Word or spreadsheet file is not searched at all by the line search. Memory (`jarvis.memory.search`) holds what JARVIS was told, not what is in the owner's folders. `P11-014` asks for local retrieval over granted folders that answers with the passage and its source, with nothing indexed that the owner did not grant.

## Decision

1. **A full-text index inside the profile database** (migration 25: `knowledge_files`, `knowledge_chunks`, and an FTS5 table ranked by BM25). Text is cut into pieces of about 1,200 characters at line ends; each piece records its line and its character offset. No new dependency: SQLite's FTS5 ships with the bundled SQLite already in use.
2. **The grant is the boundary.** The indexer walks and reads only through `WorkspaceRoots`, the same confinement handles the file tools use. Links are never followed; hidden names (a leading dot, so `.env` and `.ssh`), generated folders (`node_modules`, `target`, `dist`, `build`, virtual environments), binary files, files that are not UTF-8, text files over 1 MiB and documents over 10 MiB are left out. When two granted folders hold the same relative path, the earlier folder owns it, which is the one the file tools would read.
3. **It is derived data that can only shrink with the grant.** A pass removes entries whose file is gone or changed (read again by size and modification time), and entries whose folder is no longer granted. With no folder granted, the whole index is emptied when the daemon starts. Nothing is kept that the grant would not let a file read return.
4. **Bounded:** at most 20,000 files, 400 pieces per file, 12 folder levels, and a time budget per pass (60 seconds in the background, 8 seconds when a caller asks for a refresh). A pass that runs out of time stops and the next continues; the tool's answer says the index is incomplete or still being built.
5. **When it runs:** a pass at startup and every five minutes in the background, and `refresh: true` on the search for a file added a moment ago. The first search waits briefly for a first pass.
6. **One new tool, `jarvis.knowledge.search`:** read-only, risk 0, approval automatic, scope `files.read` (it can show nothing a file read could not). Each result gives the file's path, kind, and where the passage starts (a line for text, a character offset for documents, which is the `offset` of `jarvis.files.read_document`), with the passage fenced as untrusted data like every other outside text. At most two passages come from one file.
7. **A query is only words.** The question is split into letters and digits, each word quoted, so nothing typed can be read as search syntax. Passages with every word come first; when none has them all, passages with any word are returned.
8. **No new setting.** The index follows `tool_workspace_roots`. Removing a folder from Settings empties its entries at the next start.

## Consequences

- The index is a second copy, inside the profile database, of text from folders the owner granted. It has the same protection as the rest of the profile (private directory, backup before migration) and is not exported anywhere. It is rebuilt from the folders, so it can be deleted at any time.
- Text hidden in a file can try to instruct the model; passages are fenced and the tool description says so, and a test shows an order written in a file arrives only inside the fence.
- Matching is by words (stemming is not done), so "renewal" does not find "renews". Meaning-based search needs embeddings; the adapter exists (`ADR-0047`) and the index is built so vectors can be added beside the full-text table when a measurement shows they help (`P11-014`, second half).
- Scanned PDFs and pictures are not indexed (reading them needs `tesseract` and is slow); `jarvis.files.read_document` still reads them on request.
- Not done: a cross-project view of what memory stores about a company, a console panel showing index state, and indexing of mail.
