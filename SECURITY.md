# Security notes

TNS files are treated as hostile input. They are ZIP-like containers, not
trusted directories or XML documents.

The implementation:

* validates every fixed-header and central-record range before slicing;
* checks additions and conversions for integer overflow;
* bounds input size, entry count, name size, compressed size, writer output,
  declared output, total decoded CLI output, TIXC input/expansion, and DEFLATE
  expansion;
* requires raw DEFLATE streams to consume their complete declared payload;
* requires strict local/central metadata consistency and rejects overlapping
  local records, multi-disk metadata, and trailing bytes after the EOCD;
* rejects data-descriptor records because their post-payload boundary cannot be
  safely inferred from the current bounded parser;
* rejects unknown compression methods and unsupported TIXC/XML states with
  typed errors;
* rejects traversal, absolute, drive-qualified, backslash, control-bearing,
  and empty archive names before writing;
* writes unpacked files in a staging directory and only publishes the complete
  directory after all entries decode; supported Unix targets use no-clobber
  rename and atomic rename-exchange for --force;
* writes packed files through a same-directory temporary file, flushes it, and
  uses a final rename; existing files are refused unless --force is given;
* does not use unsafe Rust.

Tolerant mode changes metadata policy and local-directory fallback only. It
does not disable size limits or path checks, and it does not silently accept an
unknown method. Applications processing untrusted files should generally use
strict mode and lower ParseOptions limits for their workload.

--force is an explicit replacement operation. If it replaces an existing
unpack directory, the old directory is exchanged with the fully decoded stage
before cleanup on Linux, Android, Apple, and Redox targets. The portable
fallback renames the old directory to a same-parent backup and rolls it back
if publication fails. Callers needing archival retention should still rename
or back up that directory first.

The external compatibility corpus used during development is kept outside this
repository. It is not test input for releases and is not redistributed.
