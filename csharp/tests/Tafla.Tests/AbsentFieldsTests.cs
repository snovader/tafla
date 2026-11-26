using CsCheck;
using Tafla.Tests.Tables;

namespace Tafla.Tests;

/// <summary>
/// Absent fields, which <see cref="TableBuilder.SetAbsent"/> and <see cref="TableWriter.SetAbsent"/>
/// leave out of a table with the offset 0, and the fields that a reader of a later revision of a
/// table finds missing, from its first field's offset and their offsets, as SPEC.md describes. The
/// tables are those of the conformance suite's model, with fields left out.
/// </summary>
public class AbsentFieldsTests
{
    /// <summary>
    /// Tables of any layout with fields left out at every depth: each field in turn is absent when
    /// the next of the absences says so
    /// </summary>
    private static readonly Gen<Table> TablesWithAbsentFields = Gen.Select(
        Strategy.Tables.Where(table => table.Fields.Count > 0),
        Gen.Bool.Array[1, 24],
        WithAbsences
    );

    /// <summary>Tables with absent fields whose nested tables a writer writes in place</summary>
    private static readonly Gen<Table> WrittenTables = TablesWithAbsentFields.Select(Write.NestedInPlace);

    /// <summary>Tables whose first field is present, as a table with later revisions needs</summary>
    private static readonly Gen<Table> RevisedTables = TablesWithAbsentFields.Where(table => table.Fields[0] is not Absent);

    /// <summary>
    /// Floats compare by their bits. The table is read at any byte offset from an aligned address, so
    /// that its arrays are read in place both aligned and unaligned.
    /// </summary>
    [Fact]
    public void ATableWithAbsentFieldsReadsBackItsValuesAtAnyByteOffset() =>
        Gen.Select(TablesWithAbsentFields, Gen.Int[0, 7]).Check(sample =>
        {
            var (table, byteOffset) = sample;
            var bytes = Bytes.AtByteOffset(table.Bytes(), byteOffset);

            Assert.Equal(table, Read.Table(bytes.Span, table));
        });

    [Fact]
    public void AnAbsentFieldHasTheOffset0AndEveryOtherAnOffsetAfterTheOffsets() =>
        TablesWithAbsentFields.Check(table =>
        {
            var layout = table.Layout.ToTableLayout(table.Version);
            var reader = new TableReader(table.Bytes(), layout);
            var firstValue = layout.HeaderSize + table.Fields.Count * layout.OffsetSize;

            for (var index = 0; index < table.Fields.Count; index++)
            {
                var offset = reader.RootOffset(index);
                if (table.Fields[index] is Absent)
                {
                    Assert.Equal(0, offset);
                }
                else
                {
                    Assert.True(offset >= firstValue, $"Field {index} at {offset}, before {firstValue}");
                }
            }
        });

    /// <summary>
    /// Reused bytes aren't zero, so the writer writes each absent field's offset, and the bytes after
    /// the table keep their values
    /// </summary>
    [Fact]
    public void ASizerAndAWriterWriteTheBytesThatTableBuilderWritesInNewAndReusedBytes() =>
        Gen.Select(WrittenTables, Gen.Int[0, 16]).Check(sample =>
        {
            var (table, extra) = sample;
            if (Built(table) is not { } expected)
            {
                return;
            }

            Assert.Equal(expected, table.Written());
            Assert.Equal(expected, table.BuiltInPlace().ToArray());

            var reused = new byte[expected.Length + extra];
            reused.AsSpan().Fill(0xff);
            var writer = table.Sized().CreateWriter(reused);
            Write.SetFields(ref writer, table, index => index);

            Assert.Equal(expected, reused[..writer.ByteLength]);
            Assert.All(reused[expected.Length..], value => Assert.Equal(0xff, value));
        });

    [Fact]
    public void AnAbsentFieldCountsAsAFieldOfTheBuilder()
    {
        var builder = new TableBuilder(1);
        builder.SetAbsent();

        var error = Assert.Throws<BuildException>(builder.SetAbsent);
        Assert.Equal((BuildErrorKind.TooManyFields, 1), (error.Kind, error.Fields));
        Assert.Equal(new byte[4], builder.ToArray());
    }

    [Fact]
    public void AnAbsentFieldOfATableRecordedInPlaceIsWrittenAsANestedBuilderWritesIt()
    {
        var nested = new TableBuilder(3);
        nested.SetAbsent();
        nested.SetInt16(-2);
        nested.SetAbsent();
        var expected = new TableBuilder(2);
        expected.SetAbsent();
        expected.SetTable(nested);

        var builder = new TableBuilder(2);
        builder.SetAbsent();
        builder.SetTable(3, static item =>
        {
            item.SetAbsent();
            item.SetInt16(-2);
            item.SetAbsent();
        });

        var bytes = builder.ToArray();
        Assert.Equal(expected.ToArray(), bytes);

        // A nested table's absent field is at its own position, as its offset is 0
        var reader = new TableReader(bytes);
        var table = reader.RootOffset(1);
        Assert.Equal(0, reader.RootOffset(0));
        Assert.Equal(table, reader.FieldOffset(table, 0));
        Assert.Equal(-2, reader.GetInt16(reader.FieldOffset(table, 1)));
        Assert.Equal(table, reader.FieldOffset(table, 2));
    }

    /// <summary>The offset of a field written as absent outside the writer's bytes, as other fields' throw</summary>
    [Fact]
    public void AnAbsentFieldsOffsetOutsideTheBytesThrowsArgumentOutOfRangeException()
    {
        var sizer = new TableSizer(1);
        sizer.AddUInt8();
        var writer = sizer.CreateWriter();

        Assert.Throws<ArgumentOutOfRangeException>(() => writer.SetAbsent(2));
    }

    [Fact]
    public void AReaderOfALaterRevisionFindsEveryFieldAfterTheTablesOwnMissing() =>
        Gen.Select(RevisedTables, Gen.Int[1, 4]).Check(sample =>
        {
            var (table, missing) = sample;
            var revised = table with { Missing = missing };

            Assert.Equal(revised, Read.Table(table.Bytes(), revised));
        });

    /// <summary>
    /// A reader of an earlier revision reads the fields that it knows, and one that knows the rest
    /// finds them present, but those that are absent
    /// </summary>
    [Fact]
    public void AReaderOfAnEarlierRevisionReadsItsFieldsAndFindsTheOthersPresentButTheAbsentOnes() =>
        Gen.Select(RevisedTables, Gen.Int[0, int.MaxValue]).Check(sample =>
        {
            var (table, split) = sample;
            var known = 1 + split % table.Fields.Count;
            var later = table.Fields.Skip(known).ToArray();
            var earlier = table with { Fields = new Seq<Field>(table.Fields.Take(known)), Missing = later.Length };

            var read = Read.Table(table.Bytes(), earlier);

            Assert.Equal(earlier with { Missing = later.Count(field => field is Absent) }, read);
        });

    /// <summary>
    /// Overwritten or cut bytes read as values, missing fields or a decode error, or as other values
    /// than the description's, and the same within larger bytes of any content, so that no read leaves
    /// them
    /// </summary>
    [Fact]
    public void ACorruptedTableReadsAsValuesOrDecodeExceptionWithinItsBytes() =>
        Gen.Select(RevisedTables, Gen.Int[0, 4], Gen.Select(Gen.Int[0, int.MaxValue], Gen.Byte, Gen.Bool).Array[1, 3])
            .Check(sample =>
            {
                var (table, missing, corruptions) = sample;
                var bytes = table.Bytes();
                foreach (var (at, value, cut) in corruptions)
                {
                    if (bytes.Length == 0)
                    {
                        break;
                    }

                    if (cut)
                    {
                        bytes = bytes[..(at % bytes.Length)];
                    }
                    else
                    {
                        bytes[at % bytes.Length] = value;
                    }
                }

                var described = table with { Missing = missing };
                Assert.Equal(Outcome(AtStartOf(bytes, 0x00).Span, described), Outcome(AtStartOf(bytes, 0xff).Span, described));
            });

    /// <summary>
    /// Returns the table with fields left out, at every depth, of it and of the tables nested in it and
    /// in its vectors: each field in turn is absent when the next of <paramref name="absences"/> says
    /// so, in order, round and round
    /// </summary>
    private static Table WithAbsences(Table table, bool[] absences)
    {
        var next = 0;

        Table Within(Table table) => table with { Fields = new Seq<Field>(table.Fields.Select(Field)) };

        Field Field(Field field)
        {
            if (absences[next++ % absences.Length])
            {
                return new Absent();
            }

            return field switch
            {
                TableField nested => nested with { Nested = nested.Nested with { Table = Within(nested.Nested.Table) } },
                VectorField vector => new VectorField(new Seq<Nested>(vector.Items.Select(item => item with { Table = Within(item.Table) }))),
                _ => field,
            };
        }

        return Within(table);
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

    /// <summary>Returns a copy of the bytes at the start of larger bytes of <paramref name="fill"/></summary>
    private static ReadOnlyMemory<byte> AtStartOf(byte[] bytes, byte fill)
    {
        var all = new byte[bytes.Length + 16];
        all.AsSpan().Fill(fill);
        bytes.CopyTo(all, 0);

        return all.AsMemory(0, bytes.Length);
    }

    /// <summary>
    /// Reads a table as its description sets it out, and returns what it reads, the mismatch of
    /// corrupted bytes that read as another shape, or the decode error, and fails on any other
    /// exception
    /// </summary>
    private static object Outcome(ReadOnlySpan<byte> bytes, Table table)
    {
        try
        {
            return Read.Table(bytes, table);
        }
        catch (DecodeException error)
        {
            return (error.Kind, error.Position, error.ByteLength, error.Available, error.End);
        }
        catch (MismatchException error)
        {
            return error.Message;
        }
    }
}
