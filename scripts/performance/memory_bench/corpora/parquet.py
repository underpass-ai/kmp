"""A small, stdlib-only Parquet reader: enough for the public bench datasets.

MemoryAgentBench ships on Hugging Face as Parquet only, and the bench may use
nothing beyond the standard library and tiktoken. This reader covers what those
files use and refuses the rest with a typed error instead of guessing:

- the Thrift compact protocol of the footer and page headers;
- data pages v1 and v2 and dictionary pages;
- PLAIN and RLE/PLAIN dictionary encodings, RLE/bit-packed levels;
- UNCOMPRESSED, SNAPPY (decoded here) and GZIP codecs;
- BOOLEAN, INT32, INT64, FLOAT, DOUBLE and BYTE_ARRAY columns;
- nested lists (repetition levels) and structs, assembled per leaf column.

`read_columns(path, leaves)` returns one value per row for each requested leaf
path (`('metadata', 'source')`, `('questions', 'list', 'element')`); a repeated
leaf comes back as nested Python lists. Nulls and empty lists are not told
apart (both read as an empty list or None), which the bench never needs.
"""
import gzip
import struct

from .public_errors import CorpusError

MAGIC = b'PAR1'
# Physical types.
BOOLEAN, INT32, INT64, INT96, FLOAT, DOUBLE, BYTE_ARRAY, FIXED = range(8)
# Page types.
DATA_PAGE, INDEX_PAGE, DICTIONARY_PAGE, DATA_PAGE_V2 = range(4)
# Encodings.
PLAIN, PLAIN_DICTIONARY, RLE, RLE_DICTIONARY = 0, 2, 3, 8
# Codecs.
UNCOMPRESSED, SNAPPY, GZIP = 0, 1, 2
REQUIRED, OPTIONAL, REPEATED = 0, 1, 2


class ParquetUnsupported(CorpusError):
    code = 'PARQUET_UNSUPPORTED'


# --- Thrift compact protocol ----------------------------------------------------------------

class _Compact:
    """Decodes a Thrift compact struct into {field_id: value}; nested structs likewise."""

    def __init__(self, data, offset=0):
        self.data, self.pos = data, offset

    def byte(self):
        value = self.data[self.pos]
        self.pos += 1
        return value

    def varint(self):
        shift = result = 0
        while True:
            byte = self.byte()
            result |= (byte & 0x7F) << shift
            if not byte & 0x80:
                return result
            shift += 7

    def zigzag(self):
        value = self.varint()
        return (value >> 1) ^ -(value & 1)

    def binary(self):
        size = self.varint()
        value = bytes(self.data[self.pos:self.pos + size])
        self.pos += size
        return value

    def value(self, kind):
        if kind in (1, 2):  # a bool as a struct field carries its value in the type
            return kind == 1
        if kind == 3:
            return struct.unpack('b', bytes([self.byte()]))[0]
        if kind in (4, 5, 6):
            return self.zigzag()
        if kind == 7:
            value = struct.unpack('<d', self.data[self.pos:self.pos + 8])[0]
            self.pos += 8
            return value
        if kind == 8:
            return self.binary()
        if kind in (9, 10):
            return self.list()
        if kind == 11:
            return self.map()
        if kind == 12:
            return self.struct()
        raise ParquetUnsupported(f'thrift compact type {kind}')

    def list(self):
        header = self.byte()
        size, kind = header >> 4, header & 0x0F
        if size == 15:
            size = self.varint()
        if kind in (1, 2):
            return [self.byte() == 1 for _ in range(size)]
        return [self.value(kind) for _ in range(size)]

    def map(self):
        size = self.varint()
        if size == 0:
            return {}
        kinds = self.byte()
        return {self.value(kinds >> 4): self.value(kinds & 0x0F) for _ in range(size)}

    def struct(self):
        fields, last = {}, 0
        while True:
            header = self.byte()
            if header == 0:
                return fields
            delta, kind = header >> 4, header & 0x0F
            field = last + delta if delta else self.zigzag()
            fields[field] = self.value(kind)
            last = field


def thrift_struct(data, offset=0):
    """(fields, end offset) of the compact struct starting at `offset`."""
    reader = _Compact(data, offset)
    return reader.struct(), reader.pos


# --- Codecs ---------------------------------------------------------------------------------

def snappy_decompress(data):
    """Raw Snappy block format (no framing), as Parquet stores it."""
    reader = _Compact(data)
    size = reader.varint()
    out = bytearray()
    pos = reader.pos
    while pos < len(data):
        tag = data[pos]
        pos += 1
        kind = tag & 3
        if kind == 0:
            length = tag >> 2
            if length >= 60:
                extra = length - 59
                length = int.from_bytes(data[pos:pos + extra], 'little')
                pos += extra
            length += 1
            out += data[pos:pos + length]
            pos += length
            continue
        if kind == 1:
            length = ((tag >> 2) & 7) + 4
            offset = ((tag >> 5) << 8) | data[pos]
            pos += 1
        elif kind == 2:
            length = (tag >> 2) + 1
            offset = int.from_bytes(data[pos:pos + 2], 'little')
            pos += 2
        else:
            length = (tag >> 2) + 1
            offset = int.from_bytes(data[pos:pos + 4], 'little')
            pos += 4
        if offset <= 0 or offset > len(out):
            raise ParquetUnsupported('corrupt snappy stream: bad copy offset')
        start = len(out) - offset
        for index in range(length):  # copies may overlap their own output
            out.append(out[start + index])
    if len(out) != size:
        raise ParquetUnsupported(f'snappy stream decoded to {len(out)} bytes, header says {size}')
    return bytes(out)


def decompress(codec, data):
    if codec == UNCOMPRESSED:
        return bytes(data)
    if codec == SNAPPY:
        return snappy_decompress(data)
    if codec == GZIP:
        return gzip.decompress(data)
    raise ParquetUnsupported(f'codec {codec} (only uncompressed, snappy, gzip)')


# --- Encodings ------------------------------------------------------------------------------

def rle_hybrid(data, pos, end, bit_width, count):
    """`count` values of the RLE/bit-packed hybrid in data[pos:end]."""
    values = []
    width = (bit_width + 7) // 8
    mask = (1 << bit_width) - 1
    reader = _Compact(data, pos)
    while len(values) < count and reader.pos < end:
        header = reader.varint()
        if header & 1:
            groups = header >> 1
            nbytes = groups * bit_width
            chunk = int.from_bytes(data[reader.pos:reader.pos + nbytes], 'little')
            reader.pos += nbytes
            for index in range(groups * 8):
                values.append((chunk >> (index * bit_width)) & mask if bit_width else 0)
        else:
            run = header >> 1
            value = int.from_bytes(data[reader.pos:reader.pos + width], 'little') if width else 0
            reader.pos += width
            values.extend([value] * run)
    return values[:count]


def plain_values(kind, data, pos, count):
    """(values, end) of `count` PLAIN values."""
    if kind == BYTE_ARRAY:
        values = []
        for _ in range(count):
            size = int.from_bytes(data[pos:pos + 4], 'little')
            values.append(bytes(data[pos + 4:pos + 4 + size]))
            pos += 4 + size
        return values, pos
    if kind == BOOLEAN:
        return [bool((data[pos + i // 8] >> (i % 8)) & 1) for i in range(count)], pos + (count + 7) // 8
    formats = {INT32: ('<i', 4), INT64: ('<q', 8), FLOAT: ('<f', 4), DOUBLE: ('<d', 8)}
    if kind not in formats:
        raise ParquetUnsupported(f'physical type {kind}')
    fmt, size = formats[kind]
    return [struct.unpack_from(fmt, data, pos + i * size)[0] for i in range(count)], pos + count * size


# --- Schema and columns ---------------------------------------------------------------------

def _bit_width(value):
    return value.bit_length()


def leaf_levels(schema):
    """{leaf path: (max_rep, max_def, repeated_defs, physical type)} from the flat schema list."""
    leaves = {}

    def walk(index, path, rep, definition, repeated):
        node = schema[index]
        name = node[4].decode('utf-8')
        repetition = node.get(3, REQUIRED)
        if repetition != REQUIRED:
            definition += 1
        if repetition == REPEATED:
            rep += 1
            repeated = repeated + (definition,)
        path = path + (name,)
        index += 1
        children = node.get(5, 0)
        if not children:
            leaves[path] = (rep, definition, repeated, node.get(1))
            return index
        for _ in range(children):
            index = walk(index, path, rep, definition, repeated)
        return index

    index, root = 1, schema[0]
    for _ in range(root.get(5, 0)):
        index = walk(index, (), 0, 0, ())
    return leaves


def assemble(levels, values, max_rep, max_def, repeated_defs):
    """Rows of one leaf column from its (repetition, definition) levels and non-null values."""
    rows, stack, source = [], [], iter(values)
    for rep, definition in levels:
        value = next(source) if definition == max_def else None
        if max_rep == 0:
            rows.append(value)
            continue
        if rep == 0:
            root = []
            rows.append(root)
            stack = [root]
        else:
            stack = stack[:rep]
        while len(stack) < max_rep and definition >= repeated_defs[len(stack) - 1]:
            inner = []
            stack[-1].append(inner)
            stack.append(inner)
        if len(stack) == max_rep and definition >= repeated_defs[max_rep - 1]:
            stack[-1].append(value)
    return rows


class ParquetFile:
    def __init__(self, data):
        if data[:4] != MAGIC or data[-4:] != MAGIC:
            raise ParquetUnsupported('not a Parquet file (missing PAR1 magic)')
        size = int.from_bytes(data[-8:-4], 'little')
        self.data = data
        self.meta, _ = thrift_struct(data, len(data) - 8 - size)
        self.schema = self.meta[2]
        self.leaves = leaf_levels(self.schema)
        self.num_rows = self.meta[3]

    @classmethod
    def open(cls, path):
        with open(path, 'rb') as handle:
            return cls(handle.read())

    def _chunk(self, group, path):
        for chunk in group[1]:
            meta = chunk[3]
            if tuple(part.decode('utf-8') for part in meta[3]) == path:
                return meta
        raise ParquetUnsupported(f'no column {".".join(path)} in a row group')

    def _pages(self, meta):
        """(levels, values) of one column chunk."""
        max_rep, max_def, _, kind = self.leaves[tuple(p.decode('utf-8') for p in meta[3])]
        codec = meta[4]
        pos = min(meta[9], meta[11]) if meta.get(11) else meta[9]  # a dictionary page comes first
        end = pos + meta[7]
        dictionary, levels, values = None, [], []
        while pos < end and len(levels) < meta[5]:
            header, pos = thrift_struct(self.data, pos)
            body = self.data[pos:pos + header[3]]
            pos += header[3]
            page = header[1]
            if page == DICTIONARY_PAGE:
                raw = decompress(codec, body)
                dictionary, _ = plain_values(kind, raw, 0, header[7][1])
            elif page == DATA_PAGE:
                raw = decompress(codec, body)
                self._data_v1(raw, header[5], kind, max_rep, max_def, dictionary, levels, values)
            elif page == DATA_PAGE_V2:
                self._data_v2(body, header[8], codec, kind, max_rep, max_def, dictionary, levels, values)
            elif page != INDEX_PAGE:
                raise ParquetUnsupported(f'page type {page}')
        return levels, values

    @staticmethod
    def _levels(raw, pos, max_level, count, prefixed):
        if max_level == 0:
            return [0] * count, pos
        if prefixed:
            size = int.from_bytes(raw[pos:pos + 4], 'little')
            pos += 4
        else:
            size = len(raw) - pos
        return rle_hybrid(raw, pos, pos + size, _bit_width(max_level), count), pos + size

    def _values(self, raw, pos, encoding, kind, count, dictionary):
        if encoding == PLAIN:
            return plain_values(kind, raw, pos, count)[0]
        if encoding in (PLAIN_DICTIONARY, RLE_DICTIONARY):
            if dictionary is None:
                raise ParquetUnsupported('dictionary-encoded page without a dictionary page')
            width = raw[pos]
            return [dictionary[i] for i in rle_hybrid(raw, pos + 1, len(raw), width, count)]
        raise ParquetUnsupported(f'value encoding {encoding}')

    def _data_v1(self, raw, header, kind, max_rep, max_def, dictionary, levels, values):
        count = header[1]
        reps, pos = self._levels(raw, 0, max_rep, count, True)
        defs, pos = self._levels(raw, pos, max_def, count, True)
        present = sum(1 for d in defs if d == max_def)
        values.extend(self._values(raw, pos, header[2], kind, present, dictionary))
        levels.extend(zip(reps, defs))

    def _data_v2(self, body, header, codec, kind, max_rep, max_def, dictionary, levels, values):
        count, rep_len, def_len = header[1], header[6], header[5]
        reps = rle_hybrid(body, 0, rep_len, _bit_width(max_rep), count) if max_rep else [0] * count
        defs = (rle_hybrid(body, rep_len, rep_len + def_len, _bit_width(max_def), count)
                if max_def else [0] * count)
        rest = body[rep_len + def_len:]
        raw = decompress(codec, rest) if header.get(7, True) else bytes(rest)
        present = sum(1 for d in defs if d == max_def)
        values.extend(self._values(raw, 0, header[4], kind, present, dictionary))
        levels.extend(zip(reps, defs))

    def column(self, path):
        path = tuple(path)
        if path not in self.leaves:
            raise ParquetUnsupported(f'no leaf column {".".join(path)}; have {sorted(self.leaves)}')
        max_rep, max_def, repeated, kind = self.leaves[path]
        rows = []
        for group in self.meta[4]:
            levels, values = self._pages(self._chunk(group, path))
            if kind == BYTE_ARRAY:
                values = [value.decode('utf-8') for value in values]
            rows.extend(assemble(levels, values, max_rep, max_def, repeated))
        if len(rows) != self.num_rows:
            raise ParquetUnsupported(f'{".".join(path)}: {len(rows)} rows, footer says {self.num_rows}')
        return rows


def read_columns(path, leaves):
    """{leaf path tuple: [value per row]} for the requested leaves."""
    parquet = ParquetFile.open(path)
    return {tuple(leaf): parquet.column(leaf) for leaf in leaves}
