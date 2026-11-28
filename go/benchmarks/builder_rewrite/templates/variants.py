"""Patches a copy of this branch's runtime into a variant of its buffer's
growth, for setup.sh:

    python3 variants.py <package directory> grow2|appendgrow|roomfields
"""
import sys

directory, variant = sys.argv[1], sys.argv[2]


def patch(name, old, new):
    path = f'{directory}/{name}'
    source = open(path).read()
    if old not in source:
        sys.exit(f'{variant}: no match in {name}')
    open(path, 'w').write(source.replace(old, new, 1))


if variant == 'grow2':
    # The buffer grows twofold at every size, rather than fourfold below 64 KiB
    patch('builder_buffer.go', 'return max(4*capacity, firstCapacity)', 'return max(2*capacity, firstCapacity)')
elif variant == 'roomfields':
    # New bytes start with room for 64 bytes a field after the header, rather
    # than for at least 256 bytes
    patch('table_builder.go', 'return max(firstCapacity, int(uint64(header)+room))', 'return int(uint64(header) + room)')
elif variant == 'appendgrow':
    # A value that doesn't fit is appended, so that growing the buffer, as
    # append grows a slice, doesn't zero the bytes that the value fills
    patch('table_builder.go', '''	buffer, at := b.reserve(int(length) - position)
	at = b.startCounted(buffer, at, position, padding, count)
	copy(buffer[at:], bytes)
	b.setField(alignment)

	return nil
}''', '''	if at := len(b.buffer); int(length)-position > cap(b.buffer)-at {
		head := int(padding) + b.layout.offsetType.Size()
		b.buffer = append(b.buffer, make([]byte, head)...)
		b.buffer = append(b.buffer, bytes...)
		b.startCounted(b.buffer, at, position, padding, count)
		b.setField(alignment)
		return nil
	}

	buffer, at := b.reserve(int(length) - position)
	at = b.startCounted(buffer, at, position, padding, count)
	copy(buffer[at:], bytes)
	b.setField(alignment)

	return nil
}''')
else:
    sys.exit(f'unknown variant {variant}')
