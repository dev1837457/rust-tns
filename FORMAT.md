# Understood TNS format

This is an interoperability description of the subset implemented by
tns-core; it is not a claim that every TI-Nspire file variant is understood.
All integer fields below are little-endian.

## Outer container

TNS is ZIP-shaped but not always a ZIP file:

    first local record:
      "*TIMLP####"       10 bytes, normally only for the first entry
      or "PK\003\004"     4 bytes
      local fixed fields  26 bytes
      name               name_len bytes
      extra              extra_len bytes
      payload            compressed_size bytes

    ordinary central records:
      "PK\001\002"        4 bytes
      central fixed       42 bytes
      name, extra, comment

    end record:
      "TIPD"              4 bytes for normal TNS output
      or "PK\005\006"     4 bytes for ordinary ZIP-style output
      standard EOCD body  18 bytes

The local fixed fields are the familiar ZIP fields: version needed, flags,
method, DOS time/date, CRC-32, compressed size, uncompressed size, name
length, and extra length. The central fixed fields additionally carry the
made-by version, comment length, disk numbers, attributes, and local-header
offset. TNS producers differ about whether central offsets are measured with
the six-byte TIMLP decoration included; the tolerant parser tries the exact
offset and the two six-byte adjustments, while still validating each local
record and all ranges.

The parser prefers a valid central directory. In tolerant mode it can fall
back to a bounded scan of local headers when the directory or EOCD is absent.
That recovery follows declared record boundaries from byte zero, so a local
header signature embedded in a payload cannot create a phantom entry. Strict
mode requires an EOCD ending at end-of-file, a central directory ending
exactly at the EOCD, consistent single-disk counts, and matching local and
central names, flags, methods, CRCs, and sizes.

Names are decoded as UTF-8 when the ZIP UTF-8 flag is set. Tolerant decoding
uses a loss-replacing fallback for an unflagged non-UTF-8 name. Packing and
unpacking reject absolute, empty, dot/dot-dot, backslash, colon, and NUL
bearing names, as well as other control characters.

## Payload methods

* Method 0 stores the entry bytes verbatim.
* Method 8 stores a raw DEFLATE stream (no zlib wrapper). The directory CRC and
  uncompressed size describe the expanded bytes.
* Method 13 contains:

      encrypted 40-byte TIEN0100 header
      encrypted counter-mode body
        raw DEFLATE
          TIXC0100 token stream
            readable UTF-8 XML

The fixed header is decrypted with the 24-byte 3DES key used by Luna/TnsTools.
The plaintext starts with TIEN0100, has a little-endian 0x400 block-size
field, a four-byte counter seed, 21 bytes of packed key material, and three
reserved zero bytes. Each seven-byte packed DES component expands to an
eight-byte odd-parity DES key. The body keystream is 3DES-ECB over
00 00 00 00 followed by little_endian(seed + block_index mod 0x400), XORed
with the body. The final body block may be partial.

The writer uses the known interoperable defaults, raw DEFLATE level 9, and
records final XML CRC/size for method 13. MetadataStyle::Payload exists for
regression fixtures that need to reproduce Luna's older directory convention.

## TIXC0100

TIXC begins with TIXC0100-<xml-version>?>. The decoder emits the canonical
declaration:

    <?xml version="1.0" encoding="UTF-8" ?>

The grammar has a 256-entry tag dictionary and a 256-entry attribute
dictionary. First-use tag names are literal and later starts and close tags
use one-byte references where the grammar permits. The decoder accepts
attribute-name references found in input, but the encoder deliberately emits
attributes literally because TI's own expander has been observed to truncate
at repeated-attribute reference tokens. Literal and reference tag starts have
slightly different implicit greater-than states, which is why the encoder
performs an internal decode check.

Text has literal printable ASCII, two shorthand tables, compact forms for
Unicode, and several TI-specific text states. The implementation preserves
valid UTF-8 including supplementary code points such as emoji. CDATA opener,
body, and ]]> terminator bytes pass through literally. XML comments, DOCTYPE,
arbitrary processing instructions, non-canonical declarations, invalid UTF-8,
XML 1.0-forbidden code points, and malformed dictionary/control states are
rejected.

Lua packing represents source occurrences of ]]> as ]]]]><![CDATA[> across
adjacent CDATA sections before TIXC encoding. This preserves all three source
bytes while producing legal XML.

## Metadata compatibility

For method 0 and method 8, the directory fields are checked against the final
decoded bytes. For method 13 there are two observed conventions:

1. final convention: CRC/uncompressed_size are final XML;
2. legacy Luna convention: CRC/uncompressed_size are the encrypted method-13
   payload.

decode_entry reports ValidFinal, LegacyPayload, or Mismatch, and carries
warnings in tolerant mode. Strict mode rejects every method-13 interpretation
other than ValidFinal.
