using System.Buffers.Binary;
using System.Text;
using CsCheck;
using Tafla.Tests.Tables;

namespace Tafla.Tests;

/// <summary>
/// A <see cref="TableSizer"/> counts, and a <see cref="TableWriter"/> writes, the bytes that a
/// <see cref="TableBuilder"/> of the same fields writes, rejects what the builder rejects, and never
/// writes outside its bytes.
/// </summary>
public class TableWriterTests
{
    /// <summary>Tables of the offset types whose limits a test reaches: uint32's is 2 GiB</summary>
    private static readonly Gen<Table> ShortTables =
        Strategy.TablesOf(Strategy.LayoutsOf(Gen.OneOf(OffsetType.UInt8, OffsetType.UInt16))).Select(Write.NestedInPlace);

    /// <summary>Tables of any layout, whose nested tables a writer writes in place</summary>
    private static readonly Gen<Table> InPlaceTables = Strategy.Tables.Select(Write.NestedInPlace);

    [Fact]
    public void ATableSizedAndWrittenFromAnyDescriptionHoldsTheBytesThatTableBuilderWrites() =>
        InPlaceTables.Check(table =>
        {
            if (Built(table) is not { } expected)
            {
                // Tables nested in place can make it too long, which the sizer rejects as the builder does
                AssertTooLong(() => table.Sized().GetByteLength(), table);
                return;
            }

            Assert.Equal(expected.Length, table.Sized().GetByteLength());
            Assert.Equal(expected, table.Written());
        });

    [Fact]
    public void ATableWrittenIntoLongerBytesHoldsAtTheirStartWhatNewBytesHoldAndLeavesTheRest() =>
        Gen.Select(InPlaceTables, Gen.Int[0, 16], Gen.Byte).Check(sample =>
        {
            var (table, extra, fill) = sample;
            if (Built(table) is not { } expected)
            {
                return;
            }

            var bytes = new byte[expected.Length + extra];
            bytes.AsSpan().Fill(fill);

            var writer = table.Sized().CreateWriter(bytes);
            Write.SetFields(ref writer, table, index => index);

            Assert.Same(bytes, writer.Bytes);
            Assert.Equal(expected.Length, writer.ByteLength);
            Assert.Equal(expected, bytes[..expected.Length]);
            Assert.All(bytes[expected.Length..], value => Assert.Equal(fill, value));
        });

    [Fact]
    public void BytesShorterThanTheTableThrowArgumentExceptionBeforeAnyIsWritten()
    {
        var sizer = new TableSizer(1, new TableLayout { Sized = true });
        sizer.AddString("abc");

        var bytes = new byte[sizer.GetByteLength() - 1];
        bytes.AsSpan().Fill(0xa5);

        Assert.Throws<ArgumentException>(() => sizer.CreateWriter(bytes));
        Assert.All(bytes, value => Assert.Equal(0xa5, value));
        Assert.Throws<ArgumentNullException>(() => sizer.CreateWriter(null!));
    }

    /// <summary>
    /// A uint8 leaves the uint32 array 3 bytes short of its alignment, and the structs leave the nested
    /// table of a float64 array 2 bytes short of its alignment of 8, from which its own array is
    /// aligned. Vector items aren't padded.
    /// </summary>
    [Fact]
    public void ArraysStructsAndNestedTablesArePaddedAsTableBuilderPadsThem()
    {
        var builder = new TableBuilder(5);
        builder.SetUInt8(1);
        builder.SetUInt32Array([2, 3]);
        builder.SetStructArray([4, 0, 5, 0, 6, 0], 2, 2);
        var nested = new TableBuilder(1);
        nested.SetFloat64Array([7.5]);
        builder.SetTable(nested);
        var item = new TableBuilder(1);
        item.SetUInt8(8);
        builder.SetVector(item, item);

        var sizer = new TableSizer(5);
        sizer.AddUInt8();
        sizer.AddUInt32Array(2);
        sizer.AddStructArray(3, 2, 2);
        sizer.AddTable(8);
        var parent = sizer.StartTable(1);
        sizer.AddFloat64Array(1);
        sizer.EndTable(parent);
        sizer.AddVector(2);
        for (var index = 0; index < 2; index++)
        {
            var itemParent = sizer.StartTable(1);
            sizer.AddUInt8();
            sizer.EndTable(itemParent);
        }

        var writer = sizer.CreateWriter();
        writer.SetUInt8(0, 1);
        writer.SetUInt32Array(1, [2, 3]);
        writer.SetStructArray(2, [4, 0, 5, 0, 6, 0], 2, 2);
        writer.SetTable(3, 8);
        var writerParent = writer.StartTable(1);
        writer.SetFloat64Array(0, [7.5]);
        writer.EndTable(writerParent);
        var vector = writer.SetVector(4, 2);
        for (var index = 0; index < 2; index++)
        {
            var itemParent = writer.StartTable(1);
            writer.SetUInt8(0, 8);
            writer.EndTable(itemParent);
            writer.SetItemEnd(vector, index);
        }

        var bytes = writer.Bytes;
        Assert.Equal(builder.ToArray(), bytes);

        var reader = new TableReader(bytes);
        Assert.Equal(0, (reader.RootOffset(1) + sizeof(uint)) % sizeof(uint));
        Assert.Equal(0, (reader.RootOffset(2) + sizeof(uint)) % 2);
        Assert.Equal(0, reader.RootOffset(3) % sizeof(double));
        Assert.Equal([7.5], reader.GetFloat64Array(reader.FieldOffset(reader.RootOffset(3), 0)).ToArray());
    }

    [Fact]
    public void StructsReservedInPlaceHoldTheBytesOfTheStructsWrittenFromTheirBytes() =>
        Gen.Select(Gen.Byte.Array[0, 12], Gen.Byte[0, 4].Array[0, 3], Gen.OneOf(1, 2, 4, 8)).Check(sample =>
        {
            var (structBytes, prefix, alignment) = sample;
            var structs = Enumerable.Repeat(structBytes, 3).SelectMany(bytes => bytes).ToArray();
            var size = Math.Max(structBytes.Length, 1);
            var count = structBytes.Length == 0 ? 0 : 3;

            var sizer = new TableSizer(3);
            sizer.AddUInt8Array(prefix.Length);
            sizer.AddStruct(structBytes.Length);
            sizer.AddStructArray(count, size, alignment);

            var copied = sizer.CreateWriter();
            copied.SetUInt8Array(0, prefix);
            copied.SetStruct(1, structBytes);
            copied.SetStructArray(2, structs, size, alignment);

            var reserved = sizer.CreateWriter();
            reserved.SetUInt8Array(0, prefix);
            structBytes.CopyTo(reserved.ReserveStruct(1, structBytes.Length));
            structs.CopyTo(reserved.ReserveStructArray(2, count, size, alignment));

            Assert.Equal(copied.Bytes, reserved.Bytes);
        });

    /// <summary>
    /// Tables of scalars, whose sizes are fixed, written whole into the bytes reserved for them, as
    /// tafla-gen's code writes them, are the tables written field by field: a vector's items of the
    /// same scalars, sized at once, and a field's table
    /// </summary>
    [Fact]
    public void TablesReservedWholeHoldTheBytesOfThoseWrittenFieldByField() =>
        Gen.OneOf(Scalars.All).Array[1, 4].SelectMany(types =>
            Gen.ULong.Array[types.Length].Array[0, 5].Select(items => (types, items))
        ).Check(sample =>
        {
            var (types, items) = sample;
            var tables = items
                .Select(bits => new Table(
                    Layout.Headerless(OffsetType.UInt32),
                    [.. types.Select((type, index) => new Scalar(type, Scalars.Truncate(type, bits[index])))]
                ))
                .ToArray();
            var size = tables.Length > 0 ? tables[0].Bytes().Length : types.Length * sizeof(uint) + Scalars.StructSize(types);
            var field = new Table(Layout.Headerless(OffsetType.UInt32), [.. types.Select(type => new Scalar(type, 0))]);

            var builder = new TableBuilder(2);
            builder.SetVector([.. tables.Select(table => (NestedTable)table.Built())]);
            builder.SetTable(field.Built());

            var sizer = new TableSizer(2);
            sizer.AddVector(tables.Length);
            sizer.AddTableBytes(tables.Length, size);
            sizer.AddTable(1);
            sizer.AddTableBytes(1, size);

            var writer = sizer.CreateWriter();
            var vector = writer.SetVector(0, tables.Length);
            for (var index = 0; index < tables.Length; index++)
            {
                tables[index].Bytes().CopyTo(writer.ReserveTableBytes(size));
                writer.SetItemEnd(vector, index);
            }

            writer.SetTable(1, 1);
            field.Bytes().CopyTo(writer.ReserveTableBytes(size));

            Assert.Equal(builder.ToArray(), writer.Bytes);
        });

    /// <summary>
    /// The bytes of a table that its caller lays out itself, as tafla-gen's code lays out a message of
    /// fixed fields, hold the header that a writer writes, and zeros after it
    /// </summary>
    [Fact]
    public void TableBytesHoldTheHeaderThatAWriterWritesAndZerosAfterIt() =>
        Gen.Select(Strategy.Layouts, Gen.Int[0, 8], Gen.Int[0, 40], Gen.UInt).Check(sample =>
        {
            var (layout, fields, extra, anyVersion) = sample;
            var version = (uint)(anyVersion % (layout.OffsetType.MaxValue + 1L));
            var tableLayout = layout.ToTableLayout(version);
            var rest = fields * tableLayout.OffsetSize + extra;
            var length = tableLayout.HeaderSize + rest;
            if (length > tableLayout.OffsetType.MaxValue)
            {
                return;
            }

            // A writer of no fields writes the header alone, and its bytes are zero after it
            var sizer = new TableSizer(0, tableLayout);
            sizer.AddTableBytes(1, rest);
            var writer = sizer.CreateWriter();
            writer.ReserveTableBytes(rest);

            var bytes = TableWriter.CreateTableBytes(fields, tableLayout, length);

            Assert.Equal(writer.Bytes, bytes);
        });

    [Fact]
    public void TableBytesLongerThanTheOffsetTypeAddressesAreRejectedBeforeAllocating()
    {
        var uint8 = new TableLayout { OffsetType = OffsetType.UInt8 };

        Assert.Equal(255, TableWriter.CreateTableBytes(2, uint8, 255).Length);
        var tooLong = Assert.Throws<BuildException>(() => TableWriter.CreateTableBytes(2, uint8, 256));
        Assert.Equal((BuildErrorKind.TooLong, 1, 256L, OffsetType.UInt8), (tooLong.Kind, tooLong.Index, tooLong.Length, tooLong.OffsetType));
        Assert.Equal(BuildErrorKind.TooLong, Assert.Throws<BuildException>(() => TableWriter.CreateTableBytes(1, default, long.MaxValue)).Kind);

        var versioned = new TableLayout { Version = 256, OffsetType = OffsetType.UInt8 };
        Assert.Equal(BuildErrorKind.VersionTooLarge, Assert.Throws<BuildException>(() => TableWriter.CreateTableBytes(1, versioned, 8)).Kind);
        Assert.Equal(BuildErrorKind.HeaderTooLong, Assert.Throws<BuildException>(() => TableWriter.CreateTableBytes(256, uint8, 256)).Kind);
        Assert.Throws<ArgumentOutOfRangeException>(() => TableWriter.CreateTableBytes(2, uint8, 1));
        Assert.Throws<ArgumentOutOfRangeException>(() => TableWriter.CreateTableBytes(-1, uint8, 8));
    }

    /// <summary>A string's UTF-8 is counted as the writer writes it, a lone surrogate as U+FFFD's 3 bytes</summary>
    [Fact]
    public void AStringIsCountedAsItsUtf8IsWritten() =>
        Strategy.String(16).Check(value =>
        {
            var sizer = new TableSizer(1);
            sizer.AddString(value);
            var writer = sizer.CreateWriter();
            writer.SetString(0, value);

            Assert.Equal(writer.Bytes.Length - 2 * sizeof(uint), TableSizer.Utf8Length(value));
            Assert.Equal(Encoding.UTF8.GetByteCount(value), TableSizer.Utf8Length(value));
            Assert.Equal(3 + 1, TableSizer.Utf8Length("\ud800a"));
        });

    [Fact]
    public void ATableReservedPastTheTableThrowsNotAsSized()
    {
        var sizer = new TableSizer(1);
        sizer.AddVector(1);
        sizer.AddTableBytes(1, 8);

        var writer = sizer.CreateWriter();
        writer.SetVector(0, 1);

        var exception = Assert.Throws<BuildException>(() => writer.ReserveTableBytes(9));
        Assert.Equal(BuildErrorKind.NotAsSized, exception.Kind);
        Assert.Equal(8, writer.ReserveTableBytes(8).Length);
    }

    [Fact]
    public void AStructOf0BytesIsSizedAndWrittenAsTableBuilderBuildsIt()
    {
        var builder = new TableBuilder(2);
        builder.SetStruct([]);
        builder.SetUInt8(1);

        var sizer = new TableSizer(2);
        sizer.AddStruct(0);
        sizer.AddUInt8();

        var writer = sizer.CreateWriter();
        writer.SetStruct(0, []);
        writer.SetUInt8(1, 1);

        Assert.Equal(builder.ToArray(), writer.Bytes);
    }

    /// <summary>
    /// After any fields, an array of bytes as long as fits is sized and written, and a byte longer is
    /// rejected, before anything is allocated, as TableBuilder rejects it
    /// </summary>
    [Fact]
    public void TheLongestTableThatFitsIsWrittenAndAByteLongerIsRejectedAsTableBuilderRejectsIt() =>
        ShortTables.Check(table =>
        {
            var withEmpty = table with { Fields = new Seq<Field>([.. table.Fields, new ArrayField(ScalarType.UInt8, Array.Empty<ulong>())]) };
            if (Built(withEmpty) is not { } empty)
            {
                return;
            }

            var room = (int)(table.Layout.OffsetType.MaxValue - empty.Length);
            var longest = table with { Fields = new Seq<Field>([.. table.Fields, new ArrayField(ScalarType.UInt8, new ulong[room])]) };
            var tooLong = table with { Fields = new Seq<Field>([.. table.Fields, new ArrayField(ScalarType.UInt8, new ulong[room + 1])]) };

            Assert.Equal(table.Layout.OffsetType.MaxValue, (uint)longest.Sized().GetByteLength());
            Assert.Equal(longest.Bytes(), longest.Written());

            AssertTooLong(() => tooLong.Sized().GetByteLength(), tooLong);
            AssertTooLong(() => tooLong.Sized().CreateWriter(), tooLong);
            AssertTooLong(() => tooLong.Sized().CreateWriter(new byte[room * 2 + 1024]), tooLong);
            Assert.Equal(BuildErrorKind.TooLong, Assert.Throws<BuildException>(tooLong.Bytes).Kind);
        });

    [Fact]
    public void ATableOfUInt32OffsetsIsCountedPastTheLongestArrayAndRejectedBeforeAllocating()
    {
        var sizer = new TableSizer(2);
        sizer.AddUInt8Array(Array.MaxLength - 3 * sizeof(uint));
        Assert.Equal(Array.MaxLength, sizer.GetByteLength());

        // The elements after the count are padded to a multiple of 8
        sizer.AddFloat64Array(int.MaxValue);
        long elements = Array.MaxLength + sizeof(uint);
        elements += -elements & 7;

        var error = Assert.Throws<BuildException>(() => sizer.CreateWriter());
        Assert.Equal((BuildErrorKind.TooLong, 1, OffsetType.UInt32), (error.Kind, error.Index, error.OffsetType));
        Assert.Equal(elements + 8L * int.MaxValue, error.Length);
    }

    [Fact]
    public void HeadersVersionsAndFieldCountsAreCheckedAsTableBuilderChecksThem()
    {
        var versioned = new TableLayout { Version = 1, OffsetType = OffsetType.UInt8 };

        Assert.Throws<ArgumentOutOfRangeException>(() => new TableSizer(-1));

        var version = Assert.Throws<BuildException>(() => new TableSizer(1, versioned with { Version = 256 }));
        Assert.Equal((BuildErrorKind.VersionTooLarge, 256u), (version.Kind, version.Version));
        Assert.Equal(255, new TableSizer(254, versioned with { Version = 255 }).GetByteLength());

        var header = Assert.Throws<BuildException>(() => new TableSizer(255, versioned));
        Assert.Equal((BuildErrorKind.HeaderTooLong, 255), (header.Kind, header.Fields));
        Assert.Equal(BuildErrorKind.HeaderTooLong, Assert.Throws<BuildException>(() => new TableBuilder(255, versioned)).Kind);
    }

    [Fact]
    public void FieldsShorterThanThoseSizedThrowNotAsSizedFromBytesAndByteLength()
    {
        var sizer = new TableSizer(1);
        sizer.AddString("ab");

        var writer = sizer.CreateWriter();
        writer.SetString(0, "a");

        AssertNotAsSized(() => _ = writer.Bytes, 9, 10);
        AssertNotAsSized(() => _ = writer.ByteLength, 9, 10);
    }

    [Fact]
    public void FieldsThatDifferFromThoseSizedAreWrittenWhenTheyFillTheSizedBytes()
    {
        var sizer = new TableSizer(1);
        sizer.AddString("ab");

        var writer = sizer.CreateWriter();
        writer.SetString(0, "cd");

        Assert.Equal("cd", new TableReader(writer.Bytes).GetString(sizeof(uint)));
    }

    /// <summary>
    /// A writer given another field than the sizer added, into bytes of the table's length or longer,
    /// throws NotAsSized with the table's length, or writes the fields as TableBuilder does if they
    /// fill the table, since the bytes come from the fields written, and never writes past the table
    /// </summary>
    [Fact]
    public void AWriterOfOtherFieldsThanThoseSizedThrowsNotAsSizedOrWritesThemAsTableBuilderDoes() =>
        Gen.Select(InPlaceTables, Strategy.Tables, Gen.Int[0, 63], Gen.Int[0, 63], Gen.Int[0, 8], Gen.Byte).Check(sample =>
        {
            var (table, replacements, at, with, extra, fill) = sample;
            if (table.Fields.Count == 0 || replacements.Fields.Count == 0 || Built(table) is not { } sized)
            {
                return;
            }

            var fields = table.Fields.ToArray();
            fields[at % fields.Length] = replacements.Fields[with % replacements.Fields.Count];
            var written = (table with { Fields = fields }).NestedInPlace();

            var bytes = new byte[sized.Length + extra];
            bytes.AsSpan().Fill(fill);

            var exception = Record.Exception(() =>
            {
                var writer = table.Sized().CreateWriter(bytes);
                Write.SetFields(ref writer, written, index => index);
                _ = writer.Bytes;
            });

            Assert.All(bytes[sized.Length..], value => Assert.Equal(fill, value));
            if (exception is null)
            {
                Assert.Equal(written.Bytes(), bytes[..sized.Length]);
                return;
            }

            var error = Assert.IsType<BuildException>(exception);
            Assert.Equal((BuildErrorKind.NotAsSized, (long)sized.Length), (error.Kind, error.Length));
        });

    [Fact]
    public void FieldsThatEndPastTheTableThrowNotAsSized()
    {
        var sizer = new TableSizer(1);
        sizer.AddString("a");

        var past = new Action<TableWriter>[]
        {
            writer => writer.SetString(0, "abcd"),
            writer => writer.SetUInt32Array(0, [1, 2]),
            writer => writer.SetFloat64(0, 1),
            writer => writer.SetStruct(0, new byte[6]),
            writer => writer.ReserveStructArray(0, 2, 1, 1),
            writer => writer.SetVector(0, 1),
            writer => writer.StartTable(2),
        };

        foreach (var write in past)
        {
            AssertNotAsSized(() => write(sizer.CreateWriter()), 4, 9);
        }

        // A string whose count alone would end past the table
        var bytes = new TableSizer(2);
        bytes.AddUInt8();
        bytes.AddUInt8();
        var counted = bytes.CreateWriter();
        counted.SetUInt8(0, 1);
        AssertNotAsSized(() => counted.SetString(1, ""), 9, 10);
    }

    /// <summary>
    /// In bytes longer than the table, a field longer than the sizer added throws NotAsSized where it
    /// would end past the table, rather than past the bytes, and every byte after the table stays as
    /// it was
    /// </summary>
    [Fact]
    public void FieldsLongerThanThoseSizedNeverWritePastTheTableInLongerBytes()
    {
        const byte Fill = 0xAA;
        var sizer = new TableSizer(1);
        sizer.AddString("a");
        var length = sizer.GetByteLength();

        var longer = new Action<TableWriter>[]
        {
            writer => writer.SetString(0, "abcd"),
            writer => writer.SetUInt32Array(0, [1, 2]),
            writer =>
            {
                writer.SetTable(0, 1);
                writer.StartTable(1);
                writer.SetInt64(0, -1);
            },
            writer =>
            {
                var vector = writer.SetVector(0, 1);
                writer.EndTable(writer.StartTable(0));
                writer.SetItemEnd(vector, 0);
            },
        };

        foreach (var write in longer)
        {
            var bytes = new byte[32];
            bytes.AsSpan().Fill(Fill);

            var error = Assert.Throws<BuildException>(() => write(sizer.CreateWriter(bytes)));
            Assert.Equal((BuildErrorKind.NotAsSized, (long)length), (error.Kind, error.Length));
            Assert.All(bytes[length..], value => Assert.Equal(Fill, value));
        }
    }

    [Fact]
    public void NegativeCountsStructSizesBelow1AndAlignmentsOtherThan1248AreRejectedAndChangeNothing()
    {
        var sizer = new TableSizer(1);
        var rejected = new Action[]
        {
            () => sizer.StartTable(-1),
            () => sizer.AddInt8Array(-1),
            () => sizer.AddFloat64Array(-1),
            () => sizer.AddVector(-1),
            () => sizer.AddStruct(-1),
            () => sizer.AddStructArray(-1, 4, 4),
            () => sizer.AddStructArray(1, 0, 4),
            () => sizer.AddStructArray(1, 4, 3),
            () => sizer.AddTable(16),
            () => sizer.AddTableBytes(-1, 4),
            () => sizer.AddTableBytes(1, -1),
        };

        foreach (var reject in rejected)
        {
            Assert.Throws<ArgumentOutOfRangeException>(reject);
        }

        Assert.Equal(sizeof(uint), sizer.GetByteLength());

        sizer.AddStructArray(1, 4, 4);
        var writer = sizer.CreateWriter();
        var writes = new Action[]
        {
            () => writer.StartTable(-1),
            () => writer.SetVector(0, -1),
            () => writer.ReserveStruct(0, -1),
            () => writer.ReserveTableBytes(-1),
            () => writer.ReserveStructArray(0, -1, 4, 4),
            () => writer.ReserveStructArray(0, 1, 0, 4),
            () => writer.SetStructArray(0, new byte[4], 0, 4),
            () => writer.SetStructArray(0, new byte[4], 4, 3),
            () => writer.SetTable(0, 0),
        };

        foreach (var write in writes)
        {
            Assert.Throws<ArgumentOutOfRangeException>(write);
        }

        var notWhole = Assert.Throws<BuildException>(() => writer.SetStructArray(0, new byte[6], 4, 4));
        Assert.Equal((BuildErrorKind.NotWholeStructs, 6L, 4), (notWhole.Kind, notWhole.Length, notWhole.StructSize));

        writer.SetStructArray(0, [1, 2, 3, 4], 4, 4);
        var builder = new TableBuilder(1);
        builder.SetStructArray([1, 2, 3, 4], 4, 4);
        Assert.Equal(builder.ToArray(), writer.Bytes);
    }

    /// <summary>
    /// Field indices and item ends outside their tables write over other slots and fields, which
    /// leaves the table's bytes undefined, but every write stays within the writer's bytes
    /// </summary>
    [Fact]
    public void AnyIndexOfAFieldOrItemWritesWithinTheBytesOrThrowsArgumentOutOfRangeException() =>
        Gen.Select(InPlaceTables, Gen.Int[-3, 3], Gen.Int).Check(sample =>
        {
            var (table, shift, wild) = sample;
            if (Built(table) is not { } expected)
            {
                return;
            }

            var bytes = new byte[expected.Length + 8];
            var writer = table.Sized().CreateWriter(bytes);

            var exception = Record.Exception(() =>
            {
                var fields = writer;
                Write.SetFields(ref fields, table, index => index % 3 == 1 ? wild : index + shift);
            });

            Assert.True(exception is null or ArgumentOutOfRangeException, exception?.ToString());
        });

    [Fact]
    public void TheDefaultSizerAndWriterWriteATableOfNoBytes()
    {
        Assert.Equal(0, default(TableSizer).GetByteLength());
        Assert.Empty(default(TableSizer).CreateWriter().Bytes);
        Assert.Empty(default(TableWriter).Bytes);
        Assert.Equal(0, default(TableWriter).ByteLength);
    }

    [Fact]
    public void AVersionedTableHoldsItsLengthAndVersionInItsHeader()
    {
        var layout = new TableLayout { Sized = true, Version = 7, OffsetType = OffsetType.UInt16 };
        var sizer = new TableSizer(1, layout);
        sizer.AddInt64();

        var writer = sizer.CreateWriter();
        writer.SetInt64(0, -2);

        var reader = new TableReader(writer.Bytes, layout);
        Assert.Equal((14u, 7u, -2L), (reader.GetSize(), reader.GetVersion(), reader.GetInt64(reader.RootOffset(0))));
        Assert.Equal(14, BinaryPrimitives.ReadUInt16LittleEndian(writer.Bytes));
    }

    /// <summary>Returns the bytes that TableBuilder writes, or null if it rejects the table as too long</summary>
    private static byte[]? Built(Table table)
    {
        try
        {
            return table.Bytes();
        }
        catch (BuildException error) when (error.Kind == BuildErrorKind.TooLong)
        {
            return null;
        }
    }

    /// <summary>Checks that the writer throws NotAsSized, with the bytes that its fields wrote and the table's length</summary>
    private static void AssertNotAsSized(Action write, int written, int length)
    {
        var error = Assert.Throws<BuildException>(write);
        Assert.Equal((BuildErrorKind.NotAsSized, written, (long)length), (error.Kind, error.Written, error.Length));
    }

    /// <summary>Checks that the sizer rejects the table as too long, naming its last field and its length</summary>
    private static void AssertTooLong(Func<object> size, Table table)
    {
        var error = Assert.Throws<BuildException>(size);
        Assert.Equal((BuildErrorKind.TooLong, Math.Max(table.Fields.Count - 1, 0), table.Layout.OffsetType), (error.Kind, error.Index, error.OffsetType));
        Assert.True(error.Length > table.Layout.OffsetType.MaxValue);
    }
}
