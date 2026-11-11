# tafla

Binary tables of scalars, strings and typed arrays, in tafla's format.
`TableBuilder` writes a table at its exact size, copying typed arrays in bulk,
and `ByteBuffer` reads fields back by offset, with arrays as views of the
bytes. Every read checks that it stays within the bytes it was given.
`TableWriter` writes a table whose fields are known as it's written, as the
code that tafla-gen generates writes its tables, without recording them first
as `TableBuilder` does.

```pycon
>>> from array import array
>>> from tafla import ByteBuffer, TableBuilder, TableLayout

>>> LAYOUT = TableLayout(sized=True, versioned=True)  # shared by writers and readers
>>> series = TableBuilder(2)
>>> series.set_uint32_array(array("I", [0, 86400]))  # copied as bytes
>>> series.set_float32_array([21.75, 18.5])  # a list is converted first

>>> builder = TableBuilder(3, LAYOUT, 1)
>>> builder.set_float64(59.3293)
>>> builder.set_string("Europe/Stockholm")
>>> builder.set_table(series)  # written straight into the table
>>> data = builder.bytes()

>>> buffer = ByteBuffer(data, LAYOUT)
>>> buffer.get_version()
1
>>> buffer.get_string(buffer.root_offset(1))
'Europe/Stockholm'
>>> table = buffer.root_offset(2)
>>> buffer.get_float32_array(buffer.field_offset(table, 1)).tolist()
[21.75, 18.5]
```

The package's docstring, `help(tafla)`, has a longer example, with structs,
and describes the format and how each value converts to its field's type. The
tests run both examples as doctests.

The classes that the code that tafla-gen generates calls are native, written
in Rust, in the wheels for common platforms, which take about half the time
of the pure-Python ones. On other platforms, pip installs the pure-Python
wheel, which works the same way, with the same API and errors. Setting
`TAFLA_PUREPYTHON=1` uses the pure-Python classes anywhere, and
`tafla.IMPLEMENTATION` says which are in use: `"native"` or `"python"`. The
docstring's Implementations section describes the few ways in which they
differ.
[SPEC.md](https://github.com/snovader/tafla/blob/main/SPEC.md) specifies the
format, which tafla's TypeScript, Java and Rust runtimes read and write too.
