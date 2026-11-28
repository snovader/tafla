/// The fields that a `TableBuilder` records, in the order they're set: the
/// first four within the builder, and every one in an array once there are
/// more. So a small table, nested or not, allocates nothing until it's
/// written, since an array costs far more to allocate than a few fields to
/// copy.
///
/// A table nested from a builder whose fields are within it is followed by
/// those fields, which are copied, as copying the builder copies them. A
/// builder whose fields are in an array shares it with its parent instead.
struct Fields: Sendable, RandomAccessCollection {
    /// How many fields a builder records within itself
    static let inlineCount = 4

    // The fields while there are at most `inlineCount`, and `Field.unset`
    // past them, and in each once they're in `spilled`. They're properties
    // rather than a tuple, which Swift copies through its metadata.
    private var first = Field.unset
    private var second = Field.unset
    private var third = Field.unset
    private var fourth = Field.unset
    /// Every field once there are more than `inlineCount`, and empty until
    /// then
    private var spilled: [Field] = []
    private(set) var count = 0

    var startIndex: Int { 0 }

    var endIndex: Int { count }

    /// The array that holds every field, or `nil` while they're within the
    /// builder
    var array: [Field]? {
        spilled.isEmpty ? nil : spilled
    }

    subscript(position: Int) -> Field {
        guard spilled.isEmpty else {
            return spilled[position]
        }

        switch position {
        case 0: return first
        case 1: return second
        case 2: return third
        case 3: return fourth
        default: preconditionFailure("A field past the builder's")
        }
    }

    /// Appends a field, to a table of `fieldCount` fields, which the array
    /// has room for when the fields move into it, so that a table of many
    /// fields allocates it once
    mutating func append(_ field: consuming Field, fieldCount: Int) {
        guard spilled.isEmpty else {
            spilled.append(field)
            count += 1
            return
        }

        switch count {
        case 0: first = field
        case 1: second = field
        case 2: third = field
        case 3: fourth = field
        default:
            spill(reserving: fieldCount)
            spilled.append(field)
        }
        count += 1
    }

    /// Appends the fields of a nested builder, which are within it
    mutating func append(contentsOf fields: Fields, fieldCount: Int) {
        for index in fields.indices {
            append(fields[index], fieldCount: fieldCount)
        }
    }

    /// Replaces the field at `index`, which held a table recorded in place
    /// until its fields were
    mutating func replace(at index: Int, with field: consuming Field) {
        guard spilled.isEmpty else {
            spilled[index] = field
            return
        }

        switch index {
        case 0: first = field
        case 1: second = field
        case 2: third = field
        case 3: fourth = field
        default: preconditionFailure("A field past the builder's")
        }
    }

    /// Removes the fields from `index` on, of a table recorded in place that
    /// failed
    mutating func removeFields(from index: Int) {
        guard spilled.isEmpty else {
            spilled.removeSubrange(index...)
            count = index
            return
        }

        for slot in index..<count {
            replace(at: slot, with: .unset)
        }
        count = index
    }

    /// Makes room for `count` fields at once, in the array when there are
    /// more than fit within the builder
    mutating func reserveCapacity(_ count: Int) {
        guard count > Self.inlineCount else {
            return
        }

        if spilled.isEmpty {
            spill(reserving: count)
        } else {
            spilled.reserveCapacity(count)
        }
    }

    /// Moves the fields within the builder into the array, which then holds
    /// every field, with room for at least `fieldCount`, and grows as an
    /// array does
    private mutating func spill(reserving fieldCount: Int) {
        spilled.reserveCapacity(Swift.max(fieldCount, 2 * Self.inlineCount))

        if count > 0 { spilled.append(first) }
        if count > 1 { spilled.append(second) }
        if count > 2 { spilled.append(third) }
        if count > 3 { spilled.append(fourth) }
        (first, second, third, fourth) = (.unset, .unset, .unset, .unset)
    }
}

/// What a builder writes of a table besides its fields: its layout, the
/// length and version in its header, and an offset for each field
struct TableHeader: Sendable {
    /// The layout that the table is written in
    let layout: TableLayout
    /// The number of fields that the table holds, which its header's length
    /// holds too, and fits in its offsets
    let fieldCount: UInt32
    /// The byte length of the header and the offsets
    let headerLength: UInt32
    /// The table's length so far, which is where the next field goes
    var length: UInt32
}
