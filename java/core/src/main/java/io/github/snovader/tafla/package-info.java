/**
 * Binary tables of scalars, structs, strings and arrays.
 * {@link io.github.snovader.tafla.TableBuilder} writes a table at its exact size, copying arrays in
 * bulk, and {@link io.github.snovader.tafla.ByteBuffer} reads fields back by offset, into Java
 * arrays. Every read checks that it stays within the bytes it was given. The TypeScript and Python
 * implementations write and read the same bytes.
 *
 * <p>Code that knows a table's fields before it writes them, such as the code that tafla-gen
 * generates, can count the table with a {@link io.github.snovader.tafla.TableSizer} and write it
 * forward with its {@link io.github.snovader.tafla.TableWriter}, which writes the same bytes as
 * {@code TableBuilder} without recording the fields or a builder per nested table first.</p>
 *
 * {@snippet :
 * // A format's header, shared by its writer and its readers
 * TableLayout layout = TableLayout.builder().sized(true).versioned(true).build();
 * int formatVersion = 1;
 *
 * var series = new TableBuilder(2);
 * series.setUint32Array(new long[] {0, 86400});
 * series.setFloat32Array(new float[] {21.75f, 18.5f});
 *
 * var builder = new TableBuilder(3, layout, formatVersion);
 * builder.setFloat64(59.3293);
 * builder.setString("Europe/Stockholm");
 * builder.setTable(series); // written straight into the table
 * byte[] bytes = builder.bytes();
 *
 * var buffer = new ByteBuffer(bytes, layout);
 * if (buffer.getVersion() != formatVersion) {
 *     throw new IllegalStateException("Unsupported format version");
 * }
 *
 * double latitude = buffer.getFloat64(buffer.rootOffset(0));
 * String timeZone = buffer.getString(buffer.rootOffset(1));
 * int table = buffer.rootOffset(2);
 * float[] values = buffer.getFloat32Array(buffer.fieldOffset(table, 1));
 * }
 *
 * <p>Records at fixed offsets without a header or offsets, such as stream frames, are better
 * written and read with {@link java.lang.invoke.MethodHandles#byteArrayViewVarHandle}.</p>
 *
 * <h2>Format</h2>
 *
 * <p>All values are little-endian. A table is its header, one offset per field and then the fields
 * in the order they were set:</p>
 *
 * <ul>
 *   <li>The header holds the table's byte length when the layout is sized, and then its version
 *     when it's versioned.</li>
 *   <li>Header slots, offsets, counts and vector item ends are all of the layout's offset type,
 *     {@code uint8}, {@code uint16} or {@code uint32}, whose offset size of 1, 2 or 4 bytes limits
 *     the table to 255 bytes, 64 KiB or 4 GiB. A Java array holds about 2 GiB, so that's as long
 *     as a table that Java writes or reads can be.</li>
 *   <li>Offsets count from the table's start. A table nested with {@code setTable()} has no
 *     header, and its offsets count from its own start.</li>
 *   <li>Arrays and strings start with their element count. Arrays are padded so that their
 *     elements are aligned to their size from the table's start. {@code setTable()} aligns a nested
 *     table to the largest alignment of its arrays, arrays of structs and nested tables when given
 *     its builder, and to 4 bytes by default when given bytes.</li>
 *   <li>A struct is its scalars one after another, without padding, so that they lie at fixed
 *     offsets from its start. An array of structs starts with its count, and is padded so that its
 *     structs are aligned to the size of their largest scalar. {@code getStruct()} and
 *     {@code getStructArray()} return read-only views of their bytes.</li>
 *   <li>A vector starts with its item count, then the end of each item, counted from the vector's
 *     position, and then the items, one after another. {@code getVectorOffset()} finds any item
 *     from two ends, without reading the items before it.</li>
 * </ul>
 *
 * <p>Java has no unsigned types, so unsigned values are read into the next wider type, such as a
 * {@code uint32} into a {@code long}, and a {@code uint64} into a {@code long} with its bits.</p>
 *
 * <h2>Performance</h2>
 *
 * <p>Measured in OpenJDK 25 on an Apple M2 Pro, as the minimum of 41 rounds:</p>
 *
 * <ul>
 *   <li><b>Arrays of the field's Java type are copied in bulk</b>, both ways, such as a
 *     {@code float[]} for {@code float32}s. Unsigned arrays are widened or narrowed element by
 *     element.</li>
 *   <li><b>Array reads return copies</b>, since a Java array can't share the bytes' storage.
 *     The array view getters, such as {@code getFloat32ArrayView()}, return a read-only
 *     {@code FloatBuffer} or other view of the bytes instead: viewing three columns of 100,000
 *     elements took 0.04 µs and 448 bytes, against 47 µs and 1.6 MB to copy them. Alignment
 *     doesn't matter to Java, which reads at any offset, but the TypeScript implementation reads
 *     aligned arrays as views of the bytes, so Java aligns the arrays it writes.</li>
 *   <li><b>Reading elements one at a time costs no more than copying the array.</b> Summing a
 *     27,028-element {@code float32} array with {@code getFloat32()} took 6.8 µs, against 9.5 µs
 *     with {@code getFloat32Array()}, which copies it first. In TypeScript and Python, the scalar
 *     getters are the slower way.</li>
 *   <li><b>Nest builders rather than bytes.</b> {@code setTable()} and {@code setVector()} write a
 *     {@code TableBuilder} straight into its parent when the parent is written, so the outermost
 *     table allocates and writes all the bytes once. A table nested as bytes was built into an
 *     array of its own and is copied, as is a builder written already, or one whose layout has
 *     a header or another offset type than its parent's. A vector of 100 tables that each nest
 *     another took 7.1 µs to build from builders and 7.8 µs from bytes, and a table nesting
 *     100,000 rows of three columns took 54 µs and 108 µs.</li>
 *   <li><b>Small tables are cheap.</b> A table of two {@code float64}s, an {@code int64} and a
 *     16-byte string took 0.05 µs to write and 0.02 µs to read.</li>
 * </ul>
 *
 * <h2>Conversions</h2>
 *
 * <p>Each setter takes a Java type that holds its field's values: a signed field's own type, such
 * as {@code int} for an {@code int32}, the next wider type for an unsigned field, such as
 * {@code short} for a {@code uint8}, and {@code float} for a {@code float32}. A cast converts a
 * wider value to it, and the setters convert the rest:</p>
 *
 * <table>
 *   <caption>How values convert to their field's type</caption>
 *   <tr><th scope="col">Value</th><th scope="col">Written as</th></tr>
 *   <tr><td>An integer outside its field's range</td>
 *     <td>An unsigned setter writes its low bits, as a cast would: 300 as a {@code uint8} is 44,
 *     and -1 is 255. A signed setter's value is the caller's cast.</td></tr>
 *   <tr><td>A fraction or NaN for an integer field</td>
 *     <td>Not expressible: integer setters take integer types.</td></tr>
 *   <tr><td>A {@code double} too large for a {@code float32}</td>
 *     <td>The caller's cast to {@code float}, which makes it infinity of its sign and rounds
 *     other values to the nearest {@code float}.</td></tr>
 *   <tr><td>A lone surrogate in a string</td><td>U+FFFD.</td></tr>
 * </table>
 *
 * <h2>Validation and errors</h2>
 *
 * <p>Validation is always on:</p>
 *
 * <ul>
 *   <li>Every read checks its range against the bytes the {@code ByteBuffer} was given, and throws
 *     a {@link io.github.snovader.tafla.DecodeException} if it would leave them. Strings must be valid
 *     UTF-8, and a vector item must start no later than it ends. A truncated or corrupted buffer
 *     therefore throws rather than returning values from outside it.</li>
 *   <li>{@code DecodeException} is unchecked. Its message gives the read's position and length and
 *     the bytes available, and an invalid string's exception has the
 *     {@link java.nio.charset.CharacterCodingException} as its cause.</li>
 *   <li>Checks can't tell a value that was corrupted within range from a real one, such as an
 *     offset that points at another field. A format that needs that guarantee can add a
 *     checksum.</li>
 * </ul>
 *
 * <p>Using the API wrongly throws other exceptions, whatever the bytes:</p>
 *
 * <ul>
 *   <li>{@code ByteBuffer} throws an {@link IllegalArgumentException} for a struct size below 1,
 *     or below 0 for {@code getStruct()}, and {@code getVectorOffset()} an
 *     {@link IndexOutOfBoundsException} for an item outside the vector.</li>
 *   <li>{@code TableBuilder} throws an {@link IllegalStateException} for a field beyond the number
 *     it was created with, for writing before every field is set, and for a field set after
 *     writing. It throws an {@link IllegalArgumentException} for a version missing from or given
 *     for a layout, a version that doesn't fit, a nested builder with fields left to set, an
 *     alignment other than 1, 2, 4 or 8, and a struct size below 1 or bytes that aren't a whole
 *     number of structs. A field that would make the table too long for its offset type throws an
 *     {@code IllegalArgumentException} from its setter and isn't added.</li>
 *   <li>{@code TableSizer} throws the {@code IllegalArgumentException}s of {@code TableBuilder}'s
 *     constructor, and one for a negative count, a negative struct size, a struct size below 1 in
 *     an array of structs, and an alignment other than 1, 2, 4 or 8. Its {@code byteLength()} and
 *     {@code writer()} throw one for a table too long for its offset type, before allocating it.
 *     {@code TableWriter} never writes outside the sized bytes: a field that would end past them
 *     throws an {@link IndexOutOfBoundsException}, and {@code bytes()} an
 *     {@code IllegalStateException} for fields that didn't fill them. It's the target of
 *     generated code, and trusts each field's index and each item end to be its own; hand-written
 *     tables use {@code TableBuilder}.</li>
 *   <li>{@code getVersion()} throws an {@code IllegalStateException} when the layout isn't
 *     versioned.</li>
 * </ul>
 *
 * <h2>TableWriter's contract</h2>
 *
 * <p>{@code TableWriter} is the target of the generated code, whose calls follow from the schema,
 * so it doesn't check them again. Its contract:</p>
 *
 * <ul>
 *   <li>Fields are set in index order, each once, as the sizer added them, and each vector item is
 *     followed by its {@code setItemEnd()}.</li>
 *   <li>A field index outside its table, or an item outside its vector, writes an offset or item
 *     end over another slot or field, so the table's bytes are undefined. Every write still stays
 *     within the writer's bytes: one whose slot lies past them throws an
 *     {@link IndexOutOfBoundsException}.</li>
 *   <li>Fields that differ from those the sizer counted, in their lengths or number, throw an
 *     {@code IndexOutOfBoundsException} rather than end past the bytes, and {@code bytes()} an
 *     {@code IllegalStateException} when they didn't fill them.</li>
 * </ul>
 *
 * <p>Rust's writer also reports an index past its table's fields, since the parent that the
 * generated code hands back already carries the count. The others don't keep that count for each
 * nested table, so the contract doesn't promise it. Hand-written tables use {@code TableBuilder},
 * which checks every field.</p>
 */
package io.github.snovader.tafla;
