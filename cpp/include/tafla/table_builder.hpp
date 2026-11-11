// Builds a table from its fields, set one after another.

#ifndef TAFLA_TABLE_BUILDER_HPP
#define TAFLA_TABLE_BUILDER_HPP

#include <algorithm>
#include <array>
#include <bit>
#include <cstddef>
#include <cstdint>
#include <iterator>
#include <memory>
#include <optional>
#include <span>
#include <string>
#include <string_view>
#include <utility>
#include <variant>
#include <vector>

#include "bytes.hpp"
#include "element.hpp"
#include "errors.hpp"
#include "layout.hpp"
#include "table_writer.hpp"

namespace tafla {

class NestedTable;

namespace detail {

/// A value that a builder owns until it writes its table, whatever its type
struct Owned {
    Owned() = default;
    Owned(const Owned&) = delete;
    Owned& operator=(const Owned&) = delete;
    Owned(Owned&&) = delete;
    Owned& operator=(Owned&&) = delete;
    virtual ~Owned() = default;
};

template <class T>
struct OwnedValue final : Owned {
    explicit OwnedValue(T&& owned) : value(std::move(owned)) {}

    T value;
};

}  // namespace detail

/// Builds a table, which a `ByteBuffer` reads back.
///
/// A table is a header with one offset per field, after the table's total
/// length and version when its layout has them, followed by the fields in
/// the order they're set. Arrays, strings and vectors start with their
/// count, and a vector then holds the end of each item. Arrays of scalars
/// and of structs are padded so their elements are aligned from the start of
/// the table. Fields are read back by index with `ByteBuffer::root_offset()`,
/// or `ByteBuffer::field_offset()` for a table nested in another.
///
/// Setting a field records it, and `to_bytes()` then writes every field
/// through a `TableWriter`, into bytes of the table's exact length. Tables
/// nested from their builders are written by the same writer, straight into
/// this one, when they have its offset type and no header: their fields are
/// recorded among this table's when they're nested. A builder records its
/// first few fields within itself, so that a small table, nested or not,
/// allocates nothing until it's written.
///
/// A setter that takes a `std::span`, a `std::string_view` or a builder by
/// `const` reference borrows its value, which must outlive the builder's
/// last `to_bytes()`, as a view would. One that takes a `std::vector`, a
/// `std::string`, a `Bytes` or a builder by rvalue reference owns it.
///
/// A setter throws a `BuildError`, and leaves the builder as it was, when the
/// table already holds all its fields or the field would make it too long
/// for the layout's offset type: longer than 255 bytes, 64 KiB or 4 GiB.
class TableBuilder {
public:
    /// Starts a table of `fields` fields in `layout`, which its readers must
    /// use too. Nested tables use the default layout.
    ///
    /// Throws a `BuildError` if the header alone is too long for the
    /// layout's offset type.
    TableBuilder(std::size_t fields, TableLayout<false> layout)
        : TableBuilder(fields, detail::header_length(fields, layout.is_sized(), false, layout.offset_type()),
                       layout.is_sized(), std::nullopt, layout.offset_type()) {}

    /// Starts a table of `fields` fields in `layout`, with `version` in its
    /// header. Readers can check it with `ByteBuffer::version()` before
    /// reading any field.
    ///
    /// Throws a `BuildError` if `version` doesn't fit in the layout's offset
    /// type, or the header alone is too long for it.
    TableBuilder(std::size_t fields, TableLayout<true> layout, std::uint32_t version)
        : TableBuilder(fields, versioned_header(fields, layout, version), layout.is_sized(), version,
                       layout.offset_type()) {}

    TableBuilder(TableBuilder&&) noexcept;
    TableBuilder& operator=(TableBuilder&&) noexcept;
    TableBuilder(const TableBuilder&) = delete;
    TableBuilder& operator=(const TableBuilder&) = delete;
    ~TableBuilder();

    /// Writes the table, at its exact size.
    ///
    /// Throws a `BuildError` if fewer fields were set than the table holds.
    [[nodiscard]] Bytes to_bytes() const {
        check_complete();

        // The constructor checked the header and the version, and each
        // setter the table's length, so the writer fails only for a field
        // counted otherwise than it's written
        std::optional<std::uint32_t> version;
        if (versioned_) {
            version = version_;
        }

        TableWriter writer(length_, fields_, sized_, version, offset_type_);
        write_table(writer, 0, fields_);
        return writer.finish();
    }

    /// The table's byte length with the fields set so far
    [[nodiscard]] std::size_t byte_length() const noexcept {
        return length_;
    }

    /// The largest alignment that a field needs from the table's start,
    /// which `set_table()` pads it to when it's nested from its builder
    [[nodiscard]] Alignment alignment() const noexcept {
        return alignment_;
    }

    /// Sets a `std::int8_t` field. So do the other scalars' setters.
    ///
    /// Throws a `BuildError` if the table holds all its fields already, or
    /// the field would make it too long for its offset type.
    void set_int8(std::int8_t value) {
        set_scalar(value);
    }

    void set_uint8(std::uint8_t value) {
        set_scalar(value);
    }

    void set_int16(std::int16_t value) {
        set_scalar(value);
    }

    void set_uint16(std::uint16_t value) {
        set_scalar(value);
    }

    void set_int32(std::int32_t value) {
        set_scalar(value);
    }

    void set_uint32(std::uint32_t value) {
        set_scalar(value);
    }

    void set_int64(std::int64_t value) {
        set_scalar(value);
    }

    void set_uint64(std::uint64_t value) {
        set_scalar(value);
    }

    void set_float32(float value) {
        set_scalar(value);
    }

    void set_float64(double value) {
        set_scalar(value);
    }

    /// Sets an array field of `std::int8_t`, from a span that the builder
    /// borrows or a vector that it owns. So do the other arrays' setters.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_int8_array(std::span<const std::int8_t> values) {
        set_array(values);
    }

    void set_int8_array(std::vector<std::int8_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_uint8_array(std::span<const std::uint8_t> values) {
        set_array(values);
    }

    void set_uint8_array(std::vector<std::uint8_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_int16_array(std::span<const std::int16_t> values) {
        set_array(values);
    }

    void set_int16_array(std::vector<std::int16_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_uint16_array(std::span<const std::uint16_t> values) {
        set_array(values);
    }

    void set_uint16_array(std::vector<std::uint16_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_int32_array(std::span<const std::int32_t> values) {
        set_array(values);
    }

    void set_int32_array(std::vector<std::int32_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_uint32_array(std::span<const std::uint32_t> values) {
        set_array(values);
    }

    void set_uint32_array(std::vector<std::uint32_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_int64_array(std::span<const std::int64_t> values) {
        set_array(values);
    }

    void set_int64_array(std::vector<std::int64_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_uint64_array(std::span<const std::uint64_t> values) {
        set_array(values);
    }

    void set_uint64_array(std::vector<std::uint64_t>&& values) {
        set_owned_array(std::move(values));
    }

    void set_float32_array(std::span<const float> values) {
        set_array(values);
    }

    void set_float32_array(std::vector<float>&& values) {
        set_owned_array(std::move(values));
    }

    void set_float64_array(std::span<const double> values) {
        set_array(values);
    }

    void set_float64_array(std::vector<double>&& values) {
        set_owned_array(std::move(values));
    }

    /// Sets a string field, from its UTF-8 bytes, which the builder borrows
    /// from a view or owns of a string.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_string(std::string_view string) {
        const auto length = check_field(0, width() + std::uint64_t{string.size()});
        commit(bytes_field(Kind::string, string.data(), string.size()), length);
    }

    void set_string(const char* string) {
        set_string(std::string_view(string));
    }

    void set_string(std::string&& string) {
        const auto length = check_field(0, width() + std::uint64_t{string.size()});
        const auto& owned = own(std::move(string));
        commit(bytes_field(Kind::string, owned.data(), owned.size()), length);
    }

    /// Sets a struct field from its bytes: its scalars, little-endian, one
    /// after another, which are written as they are, without padding.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_struct(std::span<const std::uint8_t> bytes) {
        const auto length = check_field(0, bytes.size());
        commit(bytes_field(Kind::structure, bytes.data(), bytes.size()), length);
    }

    void set_struct(std::vector<std::uint8_t>&& bytes) {
        const auto length = check_field(0, bytes.size());
        const auto& owned = own(std::move(bytes));
        commit(bytes_field(Kind::structure, owned.data(), owned.size()), length);
    }

    /// Sets an array of structs of `size` bytes each, from their bytes one
    /// after another, which it counts. It's padded so that the structs start
    /// at a multiple of `alignment` from the start of the table, which is
    /// the size of the structs' largest scalar.
    ///
    /// Throws a `BuildError` if `size` is 0 or the bytes aren't a whole
    /// number of structs, or as the scalars' setters do.
    void set_struct_array(std::span<const std::uint8_t> bytes, std::size_t size, Alignment alignment) {
        const auto length = check_struct_array(bytes.size(), size, alignment);
        commit(struct_array_field(bytes.data(), bytes.size(), size, alignment), length);
    }

    void set_struct_array(std::vector<std::uint8_t>&& bytes, std::size_t size, Alignment alignment) {
        const auto length = check_struct_array(bytes.size(), size, alignment);
        const auto& owned = own(std::move(bytes));
        commit(struct_array_field(owned.data(), owned.size(), size, alignment), length);
    }

    /// Embeds a table, from its builder or its bytes, without a length
    /// prefix. Readers find its fields with `ByteBuffer::field_offset()` from
    /// the field's position, which reads offsets of the parent's offset type,
    /// so the table should have no header and the parent's offset type:
    /// that's the default layout in a parent of `uint32` offsets.
    ///
    /// A builder must have all its fields set. Without a header and with this
    /// table's offset type, it's written straight into this table when this
    /// table is written, without bytes of its own. Otherwise it's written
    /// into bytes of its own, in its own layout, and copied.
    ///
    /// The nested table's arrays are aligned from its own start, so it's
    /// padded to start at a multiple of an alignment, which they need to be
    /// read in place. For a builder, that's the largest alignment of its
    /// arrays, of scalars and of structs, and of its nested tables, or 1 if
    /// it has neither. For bytes, it's 4, which suits tables of 8-, 16- and
    /// 32-bit elements; the overload with an `Alignment` takes another.
    ///
    /// Throws a `BuildError` if the builder has fields left to set, or as the
    /// scalars' setters do.
    void set_table(NestedTable table);

    /// Embeds a table as the overload without an alignment does, padded to
    /// start at a multiple of `alignment`, such as `Alignment::eight` for a
    /// table of 64-bit arrays given as bytes
    void set_table(NestedTable table, Alignment alignment);

    /// Writes a vector of tables, from their builders or their bytes, after
    /// the end of each, which lets readers find any item without reading the
    /// others. Builders must have all their fields set, and are written as
    /// `set_table()` writes them: straight into this table, or into bytes of
    /// their own, in their own layout, and copied. Items aren't padded, so
    /// their arrays are read in place only when they happen to be aligned.
    ///
    /// Throws a `BuildError` if a builder has fields left to set, or as the
    /// scalars' setters do.
    void set_vector(std::vector<NestedTable> items);

private:
    friend class NestedTable;

    enum class Kind : std::uint8_t {
        scalar,
        array,
        string,
        structure,
        struct_array,
        /// A table nested in place, as a field or a vector's item, whose own
        /// fields follow it
        table,
        /// A table nested as its bytes
        table_bytes,
        /// A vector, whose items follow it
        vector,
    };

    /// A field that the builder records, to write it with the others. A table
    /// nested in place is followed by its own fields, and a vector by its
    /// items, so that a table and every table written in place in it are one
    /// sequence of fields, in the order that they're written.
    struct Field {
        Kind kind;
        /// The size of a scalar or of an array's elements
        std::uint8_t size = 0;
        /// The alignment that the field needs from its table's start
        Alignment alignment = Alignment::one;
        /// The number of an array's elements, a vector's items or the fields
        /// of a table in place, or the byte length of a string, a struct, the
        /// structs of an array or a table's bytes, which the table's length
        /// limits to 32 bits
        std::uint32_t count = 0;
        /// The size of an array's structs
        std::uint32_t struct_size = 0;
        /// A scalar's bits
        std::uint64_t bits = 0;
        /// The bytes of an array, string, struct or table
        const void* data = nullptr;
    };

    /// The fields that a builder records: the first `Inline` within it, and
    /// the others in a vector, which holds them all once they don't fit
    class Fields {
    public:
        static constexpr std::size_t Inline = 4;

        [[nodiscard]] std::size_t size() const noexcept {
            return size_;
        }

        [[nodiscard]] const Field& operator[](std::size_t index) const noexcept {
            return data()[index];
        }

        [[nodiscard]] const Field* data() const noexcept {
            return spilled_.empty() ? inline_.data() : spilled_.data();
        }

        void push_back(const Field& field) {
            if (spilled_.empty()) {
                if (size_ < Inline) {
                    inline_[size_++] = field;
                    return;
                }
                reserve(size_ + 1);
            }
            spilled_.push_back(field);
            ++size_;
        }

        /// Appends another builder's fields, copying them at once
        void append(const Fields& fields) {
            const auto size = size_ + fields.size_;
            if (spilled_.empty() && size <= Inline) {
                std::copy_n(fields.data(), fields.size_, inline_.data() + size_);
            } else {
                reserve(size);
                spilled_.insert(spilled_.end(), fields.data(), fields.data() + fields.size_);
            }
            size_ = size;
        }

        /// Makes room for `size` fields, within the builder while they fit, and
        /// otherwise in the vector, which grows at least twofold, so that
        /// fields added a few at a time take amortized constant time
        void reserve(std::size_t size) {
            if (size <= Inline) {
                return;
            }

            if (spilled_.empty()) {
                spilled_.reserve(std::max(size, 2 * Inline));
                spilled_.assign(inline_.begin(), inline_.begin() + static_cast<std::ptrdiff_t>(size_));
            } else if (size > spilled_.capacity()) {
                spilled_.reserve(std::max(size, 2 * spilled_.capacity()));
            }
        }

    private:
        std::array<Field, Inline> inline_{};
        std::vector<Field> spilled_;
        std::size_t size_ = 0;
    };

    TableBuilder(std::size_t fields, std::uint32_t header, bool sized, std::optional<std::uint32_t> version,
                 OffsetType offset_type);

    static std::uint32_t versioned_header(std::size_t fields, TableLayout<true> layout, std::uint32_t version) {
        detail::check_version(version, layout.offset_type());
        return detail::header_length(fields, layout.is_sized(), true, layout.offset_type());
    }

    /// A field of bytes, whose length the table's length check limits
    static Field bytes_field(Kind kind, const void* data, std::size_t length) noexcept {
        return {kind, 0, Alignment::one, static_cast<std::uint32_t>(length), 0, 0, data};
    }

    static Field struct_array_field(const void* data, std::size_t length, std::size_t size,
                                    Alignment alignment) noexcept {
        return {Kind::struct_array,
                0,
                alignment,
                static_cast<std::uint32_t>(length),
                static_cast<std::uint32_t>(size),
                0,
                data};
    }

    template <class T>
    static Field array_field(const T* data, std::size_t count) noexcept {
        return {Kind::array,
                static_cast<std::uint8_t>(sizeof(T)),
                static_cast<Alignment>(sizeof(T)),
                static_cast<std::uint32_t>(count),
                0,
                0,
                data};
    }

    template <class T>
    void set_scalar(T value) {
        const auto length = check_field(0, sizeof(T));
        const auto bits = std::bit_cast<detail::BitsOf<T>>(value);
        commit({Kind::scalar, static_cast<std::uint8_t>(sizeof(T)), Alignment::one, 0, 0, bits, nullptr}, length);
    }

    template <class T>
    void set_array(std::span<const T> values) {
        const auto length = check_array(values.size(), sizeof(T));
        commit(array_field(values.data(), values.size()), length);
    }

    template <class T>
    void set_owned_array(std::vector<T>&& values) {
        const auto length = check_array(values.size(), sizeof(T));
        const auto& owned = own(std::move(values));
        commit(array_field(owned.data(), owned.size()), length);
    }

    /// Keeps a value until the table is written, and returns it where it's
    /// kept, which moving the builder doesn't change
    template <class T>
    const T& own(T&& value) {
        auto owned = std::make_unique<detail::OwnedValue<T>>(std::move(value));
        const T& kept = owned->value;
        owned_.push_back(std::move(owned));
        return kept;
    }

    /// The offset size, the width of offsets, counts and lengths
    [[nodiscard]] std::uint64_t width() const noexcept {
        return offset_size(offset_type_);
    }

    /// The table's length with an array of `count` elements of `size` bytes,
    /// padded so that they're aligned to their size
    [[nodiscard]] std::uint32_t check_array(std::size_t count, std::size_t size) const {
        const auto alignment = static_cast<Alignment>(size);
        const auto padding = detail::padding<std::uint64_t>(std::uint64_t{length_} + width(), alignment);
        const auto elements = count > UINT64_MAX / size ? UINT64_MAX / 2 : std::uint64_t{count} * size;
        return check_field(padding, width() + elements);
    }

    [[nodiscard]] std::uint32_t check_struct_array(std::size_t length, std::size_t size, Alignment alignment) const {
        if (size == 0 || length % size != 0) {
            throw BuildError::not_whole_structs(length, size);
        }

        const auto padding = detail::padding<std::uint64_t>(std::uint64_t{length_} + width(), alignment);
        return check_field(padding, width() + length);
    }

    /// The table's length with a field of `byte_length` bytes after
    /// `padding`, which checking here, rather than when writing the table,
    /// reports a field too long for the layout from its own setter and leaves
    /// the builder as it was. Every offset, count, length and item end is at
    /// most the table's length, so that's the one value to check.
    [[nodiscard]] std::uint32_t check_field(std::uint64_t padding, std::uint64_t byte_length) const {
        if (set_ == fields_) {
            throw BuildError::too_many_fields(fields_);
        }

        // Each part is less than 2^63, so the sum doesn't wrap
        const std::uint64_t length = std::uint64_t{length_} + padding + std::min(byte_length, UINT64_MAX / 4);
        if (length > max_value(offset_type_)) {
            throw BuildError::too_long(set_, length, offset_type_);
        }
        return static_cast<std::uint32_t>(length);
    }

    /// Records a field of the table, which makes it `length` bytes long
    void commit(const Field& field, std::uint32_t length) {
        values_.push_back(field);
        set(length, field.alignment);
    }

    /// Counts a field of the table, which makes it `length` bytes long and
    /// needs `alignment` from its start
    void set(std::uint32_t length, Alignment alignment) noexcept {
        ++set_;
        length_ = length;
        alignment_ = std::max(alignment_, alignment);
    }

    void check_complete() const {
        if (set_ != fields_) {
            throw BuildError::incomplete(set_, fields_);
        }
    }

    /// Whether the table is written in place in a table of `offset_type`
    /// offsets: without a header, and with that offset type
    [[nodiscard]] bool in_place_in(OffsetType offset_type) const noexcept {
        return offset_type_ == offset_type && !sized_ && !versioned_;
    }

    /// Records a nested table, as a field or a vector's item: a builder
    /// written in place with its fields, which it takes the values of, and
    /// anything else as its bytes, which it keeps until the table is written
    void append_nested(NestedTable&& table, Alignment alignment);

    /// Writes `fields` fields of a table, the first of which `values_` holds
    /// at `at`, as the fields of the table that `writer` is writing, and
    /// returns where its fields, and the tables written in place in them, end
    std::size_t write_table(TableWriter& writer, std::size_t at, std::size_t fields) const;

    /// Writes a table nested in place of `fields` fields, the first at `at`,
    /// after its parent's `set_table()` or `set_vector()`
    std::size_t write_nested(TableWriter& writer, std::size_t at, std::size_t fields) const {
        const auto parent = writer.start_table(fields);
        at = write_table(writer, at, fields);
        writer.end_table(parent);
        return at;
    }

    /// Writes an array's elements after its count, little-endian
    static void write_elements(TableWriter& writer, const Field& field) noexcept;

    /// The fields that were set, and the fields of the tables nested in place
    /// in them
    Fields values_;
    /// The values that the builder owns, which its fields point into
    std::vector<std::unique_ptr<detail::Owned>> owned_;
    /// The number of fields that the table holds, which a header of `uint32`
    /// offsets limits to fewer than 2^30
    std::uint32_t fields_;
    /// The number of fields set so far
    std::uint32_t set_ = 0;
    /// The table's length so far, which is where the next field goes
    std::uint32_t length_;
    std::uint32_t version_;
    bool versioned_;
    bool sized_;
    OffsetType offset_type_;
    Alignment alignment_ = Alignment::one;
};

/// A table to nest with `TableBuilder::set_table()` or in
/// `TableBuilder::set_vector()`: a builder or bytes, which it borrows from a
/// `const` reference or span and owns from an rvalue.
///
/// A builder with its parent's offset type and no header is written straight
/// into its parent. Bytes, and other builders, are copied.
class NestedTable {
public:
    // NOLINTBEGIN(google-explicit-constructor): each converts as an argument
    NestedTable(TableBuilder&& builder) : table_(std::move(builder)) {}

    NestedTable(const TableBuilder& builder) noexcept : table_(&builder) {}

    NestedTable(std::span<const std::uint8_t> bytes) noexcept : table_(bytes) {}

    NestedTable(const std::vector<std::uint8_t>& bytes) noexcept : table_(std::span<const std::uint8_t>(bytes)) {}

    NestedTable(const Bytes& bytes) noexcept : table_(bytes.span()) {}

    NestedTable(std::vector<std::uint8_t>&& bytes) noexcept : table_(std::move(bytes)) {}

    NestedTable(Bytes&& bytes) noexcept : table_(std::move(bytes)) {}
    // NOLINTEND(google-explicit-constructor)

private:
    friend class TableBuilder;

    [[nodiscard]] const TableBuilder* builder() const noexcept {
        if (const auto* borrowed = std::get_if<const TableBuilder*>(&table_)) {
            return *borrowed;
        }
        return std::get_if<TableBuilder>(&table_);
    }

    /// The bytes of a table nested as bytes, which a builder has none of
    [[nodiscard]] std::span<const std::uint8_t> bytes() const noexcept {
        if (const auto* borrowed = std::get_if<std::span<const std::uint8_t>>(&table_)) {
            return *borrowed;
        }
        if (const auto* owned = std::get_if<Bytes>(&table_)) {
            return owned->span();
        }
        if (const auto* owned = std::get_if<std::vector<std::uint8_t>>(&table_)) {
            return *owned;
        }
        return {};
    }

    [[nodiscard]] std::size_t byte_length() const noexcept {
        const auto* table = builder();
        return table != nullptr ? table->length_ : bytes().size();
    }

    [[nodiscard]] Alignment default_alignment() const noexcept {
        const auto* table = builder();
        return table != nullptr ? table->alignment_ : Alignment::four;
    }

    void check_complete() const {
        if (const auto* table = builder()) {
            table->check_complete();
        }
    }

    /// The fields that recording the table in a table of `offset_type`
    /// offsets adds: the table, and its fields when it's written in place
    [[nodiscard]] std::size_t recorded_fields(OffsetType offset_type) const noexcept {
        const auto* table = builder();
        return table != nullptr && table->in_place_in(offset_type) ? 1 + table->values_.size() : 1;
    }

    std::variant<const TableBuilder*, TableBuilder, std::span<const std::uint8_t>, Bytes, std::vector<std::uint8_t>>
        table_;
};

inline TableBuilder::TableBuilder(std::size_t fields, std::uint32_t header, bool sized,
                                  std::optional<std::uint32_t> version, OffsetType offset_type)
    // The header holds an offset of at least a byte per field
    : fields_(static_cast<std::uint32_t>(fields)),
      length_(header),
      version_(version.value_or(0)),
      versioned_(version.has_value()),
      sized_(sized),
      offset_type_(offset_type) {
    values_.reserve(fields);
}

inline TableBuilder::TableBuilder(TableBuilder&&) noexcept = default;
inline TableBuilder& TableBuilder::operator=(TableBuilder&&) noexcept = default;
inline TableBuilder::~TableBuilder() = default;

inline void TableBuilder::set_table(NestedTable table) {
    const auto alignment = table.default_alignment();
    set_table(std::move(table), alignment);
}

inline void TableBuilder::set_table(NestedTable table, Alignment alignment) {
    table.check_complete();

    // Padded from the table's length, unlike an array, since it has no count
    const std::uint64_t padding = detail::padding<std::uint64_t>(length_, alignment);
    const auto length = check_field(padding, table.byte_length());

    values_.reserve(values_.size() + table.recorded_fields(offset_type_));
    append_nested(std::move(table), alignment);
    set(length, alignment);
}

inline void TableBuilder::set_vector(std::vector<NestedTable> items) {
    // In 64 bits, which only a vector that nests a builder many times can
    // overflow, saturating far past the longest table
    std::uint64_t nested_bytes = 0;
    std::size_t recorded = 1;
    for (const auto& item : items) {
        item.check_complete();
        nested_bytes = std::min<std::uint64_t>(nested_bytes + item.byte_length(), UINT64_MAX / 4);
        recorded += item.recorded_fields(offset_type_);
    }

    const auto slots = std::min<std::uint64_t>(std::uint64_t{items.size()} + 1, UINT64_MAX / 16) * width();
    const auto length = check_field(0, std::min(slots + nested_bytes, UINT64_MAX / 4));

    values_.reserve(values_.size() + recorded);
    values_.push_back({Kind::vector, 0, Alignment::one, static_cast<std::uint32_t>(items.size()), 0, 0, nullptr});
    for (auto& item : items) {
        append_nested(std::move(item), Alignment::one);
    }
    set(length, Alignment::one);
}

inline void TableBuilder::append_nested(NestedTable&& table, Alignment alignment) {
    if (auto* owned = std::get_if<TableBuilder>(&table.table_); owned != nullptr && owned->in_place_in(offset_type_)) {
        values_.push_back({Kind::table, 0, alignment, owned->fields_, 0, 0, nullptr});
        values_.append(owned->values_);
        std::move(owned->owned_.begin(), owned->owned_.end(), std::back_inserter(owned_));
        return;
    }

    if (const auto* const* borrowed = std::get_if<const TableBuilder*>(&table.table_);
        borrowed != nullptr && (*borrowed)->in_place_in(offset_type_)) {
        // Its fields point into what it owns, which outlives this builder's
        // last `to_bytes()`
        values_.push_back({Kind::table, 0, alignment, (*borrowed)->fields_, 0, 0, nullptr});
        values_.append((*borrowed)->values_);
        return;
    }

    // Other builders are written into bytes of their own, in their own
    // layout, and bytes that the table owns are kept by this builder
    std::span<const std::uint8_t> bytes;
    if (const auto* builder = table.builder()) {
        bytes = own(builder->to_bytes()).span();
    } else if (auto* owned_bytes = std::get_if<Bytes>(&table.table_)) {
        bytes = own(std::move(*owned_bytes)).span();
    } else if (auto* owned_vector = std::get_if<std::vector<std::uint8_t>>(&table.table_)) {
        bytes = own(std::move(*owned_vector));
    } else {
        bytes = table.bytes();
    }
    values_.push_back({Kind::table_bytes, 0, alignment, static_cast<std::uint32_t>(bytes.size()), 0, 0, bytes.data()});
}

inline std::size_t TableBuilder::write_table(TableWriter& writer, std::size_t at, std::size_t fields) const {
    for (std::size_t index = 0; index < fields; ++index) {
        const Field field = values_[at++];

        switch (field.kind) {
            case Kind::scalar:
                writer.set_offset(index);
                switch (field.size) {
                    case 1:
                        writer.push_scalar(static_cast<std::uint8_t>(field.bits));
                        break;
                    case 2:
                        writer.push_scalar(static_cast<std::uint16_t>(field.bits));
                        break;
                    case 4:
                        writer.push_scalar(static_cast<std::uint32_t>(field.bits));
                        break;
                    default:
                        writer.push_scalar(field.bits);
                        break;
                }
                break;
            case Kind::array:
                writer.start_array(index, field.count, field.alignment);
                write_elements(writer, field);
                break;
            case Kind::string:
                writer.set_string(index, {static_cast<const char*>(field.data), field.count});
                break;
            case Kind::structure:
                writer.set_struct(index, {static_cast<const std::uint8_t*>(field.data), field.count});
                break;
            case Kind::struct_array:
                writer.set_struct_array(index, {static_cast<const std::uint8_t*>(field.data), field.count},
                                        field.struct_size, field.alignment);
                break;
            case Kind::table:
                writer.set_table(index, field.alignment);
                at = write_nested(writer, at, field.count);
                break;
            case Kind::table_bytes:
                writer.set_table(index, field.alignment);
                writer.copy_table({static_cast<const std::uint8_t*>(field.data), field.count});
                break;
            case Kind::vector: {
                const auto vector = writer.set_vector(index, field.count);
                for (std::size_t item = 0; item < field.count; ++item) {
                    const Field table = values_[at++];
                    if (table.kind == Kind::table) {
                        at = write_nested(writer, at, table.count);
                    } else {
                        writer.copy_table({static_cast<const std::uint8_t*>(table.data), table.count});
                    }
                    writer.set_item_end(vector, item);
                }
                break;
            }
        }
    }
    return at;
}

inline void TableBuilder::write_elements(TableWriter& writer, const Field& field) noexcept {
    const std::size_t byte_length = std::size_t{field.count} * field.size;
    if (detail::kHostOrder || field.size == 1) {
        writer.push(field.data, byte_length);
        return;
    }

    switch (field.size) {
        case 2:
            writer.push_elements(static_cast<const std::uint16_t*>(field.data), field.count);
            return;
        case 4:
            writer.push_elements(static_cast<const std::uint32_t*>(field.data), field.count);
            return;
        default:
            writer.push_elements(static_cast<const std::uint64_t*>(field.data), field.count);
            return;
    }
}

}  // namespace tafla

#endif
