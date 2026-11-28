extension TableBuilder {
    /// A table that a ``TableBuilder`` records in place: one of its fields,
    /// with ``TableBuilder/setTable(fields:_:)``, or an item of a vector,
    /// with ``TableBuilder/setVector(count:fields:_:)``, which give it to the
    /// closure that sets its fields. Its setters are the builder's, which
    /// take the same values and throw the same errors, and record each field
    /// straight among the builder's, so the table needs no builder of its
    /// own. They count the table's own fields and length, as a builder of it
    /// would.
    ///
    /// An item comes only from that closure, and can't be copied, so it can't
    /// be kept after the closure returns. Meanwhile, the builder is used only
    /// through the item, since Swift's exclusive access to the builder keeps
    /// the closure from using it.
    public struct Item: ~Copyable {
        /// The builder, which counts the table's fields while the closure
        /// runs
        private let builder: UnsafeMutablePointer<TableBuilder>

        init(builder: UnsafeMutablePointer<TableBuilder>) {
            self.builder = builder
        }

        /// Sets an `int8` field, as ``TableBuilder/setInt8(_:)`` does.
        public mutating func setInt8(_ value: Int8) throws(BuildError) {
            try builder.pointee.setInt8(value)
        }

        /// Sets a `uint8` field, as ``TableBuilder/setUInt8(_:)`` does.
        public mutating func setUInt8(_ value: UInt8) throws(BuildError) {
            try builder.pointee.setUInt8(value)
        }

        /// Sets an `int16` field, as ``TableBuilder/setInt16(_:)`` does.
        public mutating func setInt16(_ value: Int16) throws(BuildError) {
            try builder.pointee.setInt16(value)
        }

        /// Sets a `uint16` field, as ``TableBuilder/setUInt16(_:)`` does.
        public mutating func setUInt16(_ value: UInt16) throws(BuildError) {
            try builder.pointee.setUInt16(value)
        }

        /// Sets an `int32` field, as ``TableBuilder/setInt32(_:)`` does.
        public mutating func setInt32(_ value: Int32) throws(BuildError) {
            try builder.pointee.setInt32(value)
        }

        /// Sets a `uint32` field, as ``TableBuilder/setUInt32(_:)`` does.
        public mutating func setUInt32(_ value: UInt32) throws(BuildError) {
            try builder.pointee.setUInt32(value)
        }

        /// Sets an `int64` field, as ``TableBuilder/setInt64(_:)`` does.
        public mutating func setInt64(_ value: Int64) throws(BuildError) {
            try builder.pointee.setInt64(value)
        }

        /// Sets a `uint64` field, as ``TableBuilder/setUInt64(_:)`` does.
        public mutating func setUInt64(_ value: UInt64) throws(BuildError) {
            try builder.pointee.setUInt64(value)
        }

        /// Sets a `float32` field, as ``TableBuilder/setFloat32(_:)`` does.
        public mutating func setFloat32(_ value: Float) throws(BuildError) {
            try builder.pointee.setFloat32(value)
        }

        /// Sets a `float64` field, as ``TableBuilder/setFloat64(_:)`` does.
        public mutating func setFloat64(_ value: Double) throws(BuildError) {
            try builder.pointee.setFloat64(value)
        }

        /// Sets the scalar field of type `T`, as
        /// ``TableBuilder/setScalar(_:)`` does.
        public mutating func setScalar<T: ArrayElement>(_ value: T) throws(BuildError) {
            try builder.pointee.setScalar(value)
        }

        /// Sets an array field of `int8`s, as
        /// ``TableBuilder/setInt8Array(_:)`` does.
        public mutating func setInt8Array(_ values: [Int8]) throws(BuildError) {
            try builder.pointee.setInt8Array(values)
        }

        /// Sets an array field of `uint8`s, as
        /// ``TableBuilder/setUInt8Array(_:)`` does.
        public mutating func setUInt8Array(_ values: [UInt8]) throws(BuildError) {
            try builder.pointee.setUInt8Array(values)
        }

        /// Sets an array field of `int16`s, as
        /// ``TableBuilder/setInt16Array(_:)`` does.
        public mutating func setInt16Array(_ values: [Int16]) throws(BuildError) {
            try builder.pointee.setInt16Array(values)
        }

        /// Sets an array field of `uint16`s, as
        /// ``TableBuilder/setUInt16Array(_:)`` does.
        public mutating func setUInt16Array(_ values: [UInt16]) throws(BuildError) {
            try builder.pointee.setUInt16Array(values)
        }

        /// Sets an array field of `int32`s, as
        /// ``TableBuilder/setInt32Array(_:)`` does.
        public mutating func setInt32Array(_ values: [Int32]) throws(BuildError) {
            try builder.pointee.setInt32Array(values)
        }

        /// Sets an array field of `uint32`s, as
        /// ``TableBuilder/setUInt32Array(_:)`` does.
        public mutating func setUInt32Array(_ values: [UInt32]) throws(BuildError) {
            try builder.pointee.setUInt32Array(values)
        }

        /// Sets an array field of `int64`s, as
        /// ``TableBuilder/setInt64Array(_:)`` does.
        public mutating func setInt64Array(_ values: [Int64]) throws(BuildError) {
            try builder.pointee.setInt64Array(values)
        }

        /// Sets an array field of `uint64`s, as
        /// ``TableBuilder/setUInt64Array(_:)`` does.
        public mutating func setUInt64Array(_ values: [UInt64]) throws(BuildError) {
            try builder.pointee.setUInt64Array(values)
        }

        /// Sets an array field of `float32`s, as
        /// ``TableBuilder/setFloat32Array(_:)`` does.
        public mutating func setFloat32Array(_ values: [Float]) throws(BuildError) {
            try builder.pointee.setFloat32Array(values)
        }

        /// Sets an array field of `float64`s, as
        /// ``TableBuilder/setFloat64Array(_:)`` does.
        public mutating func setFloat64Array(_ values: [Double]) throws(BuildError) {
            try builder.pointee.setFloat64Array(values)
        }

        /// Sets the array field of `T`, as ``TableBuilder/setArray(_:)``
        /// does.
        public mutating func setArray<T: ArrayElement>(_ values: [T]) throws(BuildError) {
            try builder.pointee.setArray(values)
        }

        /// Sets a string field, as ``TableBuilder/setString(_:)`` does.
        public mutating func setString(_ value: String) throws(BuildError) {
            try builder.pointee.setString(value)
        }

        /// Sets a vector of strings, as ``TableBuilder/setStringVector(_:)``
        /// does.
        public mutating func setStringVector(_ strings: [String]) throws(BuildError) {
            try builder.pointee.setStringVector(strings)
        }

        /// Sets a struct field from its bytes, as
        /// ``TableBuilder/setStruct(_:)`` does.
        public mutating func setStruct(_ bytes: [UInt8]) throws(BuildError) {
            try builder.pointee.setStruct(bytes)
        }

        /// Sets an array of structs from their bytes, as
        /// ``TableBuilder/setStructArray(_:size:alignment:)`` does.
        public mutating func setStructArray(_ bytes: [UInt8], size: Int, alignment: Alignment) throws(BuildError) {
            try builder.pointee.setStructArray(bytes, size: size, alignment: alignment)
        }

        /// Embeds a table, from its builder or its bytes, as
        /// ``TableBuilder/setTable(_:alignment:)`` does.
        public mutating func setTable(_ table: NestedTable, alignment: Alignment? = nil) throws(BuildError) {
            try builder.pointee.setTable(table, alignment: alignment)
        }

        /// Writes a vector of tables, from their builders or their bytes, as
        /// ``TableBuilder/setVector(_:)`` does.
        public mutating func setVector(_ items: [NestedTable]) throws(BuildError) {
            try builder.pointee.setVector(items)
        }

        /// Records a table nested in this one in place, as
        /// ``TableBuilder/setTable(fields:_:)`` does.
        public mutating func setTable(fields: Int, _ fill: (inout Item) throws(BuildError) -> Void) throws(BuildError) {
            try builder.pointee.setTable(fields: fields, fill)
        }

        /// Records a vector of tables in place, as
        /// ``TableBuilder/setVector(count:fields:_:)`` does.
        public mutating func setVector(
            count: Int,
            fields: Int,
            _ fill: (Int, inout Item) throws(BuildError) -> Void
        ) throws(BuildError) {
            try builder.pointee.setVector(count: count, fields: fields, fill)
        }
    }
}
