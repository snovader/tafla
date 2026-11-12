// Builds a table from its fields, set one after another.

#ifndef TAFLA_TABLE_BUILDER_HPP
#define TAFLA_TABLE_BUILDER_HPP

#include <algorithm>
#include <array>
#include <bit>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <iterator>
#include <memory>
#include <optional>
#include <ranges>
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
#include "table_length.hpp"
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

/// A container that a setter would view after the call that it's given to
/// ends, such as a `std::array` or a `const std::vector` returned by value,
/// which the builder can't own. A setter that borrows its value deletes the
/// overload of these, so that a call with one doesn't compile, where it would
/// otherwise convert to a `std::span` or `std::string_view` of it. Views,
/// such as spans, and containers that a setter owns from an rvalue are given
/// as they are.
template <class Range>
concept Temporary = std::ranges::contiguous_range<Range> && !std::ranges::borrowed_range<Range>;

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
/// A setter that takes a `std::span` or a `std::string_view` borrows its
/// value, which must outlive the builder's last `to_bytes()`, as a view
/// would. One that takes a `std::vector`, a `std::string`, a `Bytes` or a
/// builder by rvalue reference owns it. A call with a temporary of another
/// container, such as a `std::array` returned by value, doesn't compile,
/// since the builder would view it after it's destroyed. A builder nested by
/// `const` reference is recorded as it is when it's nested, as
/// `set_table()` describes, so it needn't outlive this one.
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

    /// Moves `other`'s fields and what it owns, and leaves it as a builder
    /// that was just started, with its layout and number of fields, which
    /// throws a `BuildError` until every field is set again
    TableBuilder(TableBuilder&& other) noexcept;
    TableBuilder& operator=(TableBuilder&& other) noexcept;
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
    /// borrows or a vector that it owns. So do the other arrays' setters,
    /// which don't take other temporaries, as `detail::Temporary` describes.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_int8_array(std::span<const std::int8_t> values) {
        set_array(values);
    }

    void set_int8_array(std::vector<std::int8_t>&& values) {
        set_owned_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_int8_array(Values&& values) = delete;

    void set_uint8_array(std::span<const std::uint8_t> values) {
        set_array(values);
    }

    void set_uint8_array(std::vector<std::uint8_t>&& values) {
        set_owned_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_uint8_array(Values&& values) = delete;

    void set_int16_array(std::span<const std::int16_t> values) {
        set_array(values);
    }

    void set_int16_array(std::vector<std::int16_t>&& values) {
        set_owned_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_int16_array(Values&& values) = delete;

    void set_uint16_array(std::span<const std::uint16_t> values) {
        set_array(values);
    }

    void set_uint16_array(std::vector<std::uint16_t>&& values) {
        set_owned_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_uint16_array(Values&& values) = delete;

    void set_int32_array(std::span<const std::int32_t> values) {
        set_array(values);
    }

    void set_int32_array(std::vector<std::int32_t>&& values) {
        set_owned_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_int32_array(Values&& values) = delete;

    void set_uint32_array(std::span<const std::uint32_t> values) {
        set_array(values);
    }

    void set_uint32_array(std::vector<std::uint32_t>&& values) {
        set_owned_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_uint32_array(Values&& values) = delete;

    void set_int64_array(std::span<const std::int64_t> values) {
        set_array(values);
    }

    void set_int64_array(std::vector<std::int64_t>&& values) {
        set_owned_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_int64_array(Values&& values) = delete;

    void set_uint64_array(std::span<const std::uint64_t> values) {
        set_array(values);
    }

    void set_uint64_array(std::vector<std::uint64_t>&& values) {
        set_owned_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_uint64_array(Values&& values) = delete;

    void set_float32_array(std::span<const float> values) {
        set_array(values);
    }

    void set_float32_array(std::vector<float>&& values) {
        set_owned_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_float32_array(Values&& values) = delete;

    void set_float64_array(std::span<const double> values) {
        set_array(values);
    }

    void set_float64_array(std::vector<double>&& values) {
        set_owned_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_float64_array(Values&& values) = delete;

    /// Sets a string field, from its UTF-8 bytes, which the builder borrows
    /// from a view or owns of a string.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_string(std::string_view string) {
        const auto length = check_field(detail::saturating_sum(width(), string.size()));
        commit(bytes_field(Kind::string, string.data(), string.size()), length);
    }

    void set_string(const char* string) {
        set_string(std::string_view(string));
    }

    void set_string(std::string&& string) {
        const auto length = check_field(detail::saturating_sum(width(), string.size()));
        const auto& owned = own(std::move(string));
        commit(bytes_field(Kind::string, owned.data(), owned.size()), length);
    }

    template <detail::Temporary String>
    void set_string(String&& string) = delete;

    /// Sets a struct field from its bytes: its scalars, little-endian, one
    /// after another, which are written as they are, without padding.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_struct(std::span<const std::uint8_t> bytes) {
        const auto length = check_field(bytes.size());
        commit(bytes_field(Kind::structure, bytes.data(), bytes.size()), length);
    }

    void set_struct(std::vector<std::uint8_t>&& bytes) {
        const auto length = check_field(bytes.size());
        const auto& owned = own(std::move(bytes));
        commit(bytes_field(Kind::structure, owned.data(), owned.size()), length);
    }

    template <detail::Temporary Range>
    void set_struct(Range&& bytes) = delete;

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

    template <detail::Temporary Range>
    void set_struct_array(Range&& bytes, std::size_t size, Alignment alignment) = delete;

    /// Embeds a table, from its builder or its bytes, without a length
    /// prefix. Readers find its fields with `ByteBuffer::field_offset()` from
    /// the field's position, which reads offsets of the parent's offset type,
    /// so the table should have no header and the parent's offset type:
    /// that's the default layout in a parent of `uint32` offsets.
    ///
    /// A builder must have all its fields set. Without a header and with this
    /// table's offset type, it's written straight into this table when this
    /// table is written, without bytes of its own: its fields are recorded
    /// among this table's, and this builder takes what it owns, from an
    /// rvalue, or shares it, from a `const` reference. Otherwise it's written
    /// into bytes of its own, in its own layout, at once, which this builder
    /// keeps. Either way, the nested builder may then change or be destroyed;
    /// only the values that it borrows must outlive this builder's last
    /// `to_bytes()`.
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
    /// scalars' setters do, and leaves the builder as it was, as it does if
    /// writing a builder of another layout or an allocation fails.
    void set_vector(std::vector<NestedTable> items);

    class Item;

    /// Records a table of `fields` fields nested in this one, in place, as a
    /// nested builder would be, but without a builder of its own: `fill` sets
    /// its fields, in order, on the `Item` that it's given, and each is
    /// recorded straight among this builder's fields. The table is padded to
    /// start at a multiple of its largest alignment, as `set_table()` pads a
    /// builder.
    ///
    /// The builder itself mustn't be used within `fill`, only the `Item`.
    ///
    /// Throws a `BuildError` if `fill` leaves fields of the table unset, or as
    /// the setters do, and passes on what `fill` throws, leaving the builder
    /// as it was either way.
    template <std::invocable<Item&> Fill>
    void set_table(std::size_t fields, Fill&& fill);

    /// Records a vector of `count` tables of `fields` fields each, in place,
    /// as `set_table(fields, fill)` records one: `fill(index, item)` sets the
    /// fields of item `index`, for each in order. Items aren't padded, as
    /// `set_vector()` doesn't pad them.
    ///
    /// Throws a `BuildError` if the vector's count makes the table too long
    /// before any item is recorded, if `fill` leaves an item's fields unset,
    /// or as the setters do, and passes on what `fill` throws, leaving the
    /// builder as it was either way.
    template <std::invocable<std::size_t, Item&> Fill>
    void set_vector(std::size_t count, std::size_t fields, Fill&& fill);

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
        /// The number of an array's structs
        std::uint32_t structs = 0;
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

        Fields() = default;

        /// Leaves `other` without fields, since its vector is moved, and its
        /// size would otherwise count fields that its array doesn't hold
        Fields(Fields&& other) noexcept
            : inline_(other.inline_), spilled_(std::move(other.spilled_)), size_(std::exchange(other.size_, 0)) {
            other.spilled_.clear();
        }

        Fields& operator=(Fields&& other) noexcept {
            if (this != &other) {
                inline_ = other.inline_;
                spilled_ = std::move(other.spilled_);
                size_ = std::exchange(other.size_, 0);
                other.spilled_.clear();
            }
            return *this;
        }

        Fields(const Fields&) = delete;
        Fields& operator=(const Fields&) = delete;
        ~Fields() = default;

        [[nodiscard]] std::size_t size() const noexcept {
            return size_;
        }

        [[nodiscard]] const Field& operator[](std::size_t index) const noexcept {
            return data()[index];
        }

        [[nodiscard]] const Field* data() const noexcept {
            return spilled_.empty() ? inline_.data() : spilled_.data();
        }

        /// Appends a field, inline while the array or the vector has room for
        /// it, which is every field of a small table and most of a large one,
        /// so that setting one doesn't call a function
        TAFLA_ALWAYS_INLINE void push_back(const Field& field) {
            if (spilled_.empty()) {
                if (size_ < Inline) [[likely]] {
                    inline_[size_++] = field;
                    return;
                }
            } else if (spilled_.size() < spilled_.capacity()) [[likely]] {
                spilled_.push_back(field);
                ++size_;
                return;
            }
            push_back_spilled(field);
        }

        /// Appends a field to the vector, moving the array's fields into it
        /// first when it's empty
        void push_back_spilled(const Field& field) {
            if (spilled_.empty()) {
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

        /// Sets the alignment of the field at `index`, a table recorded in
        /// place, which is known once its fields are
        void set_alignment(std::size_t index, Alignment alignment) noexcept {
            (spilled_.empty() ? inline_.data() : spilled_.data())[index].alignment = alignment;
        }

        /// Removes the fields past the first `size`, which leaves the
        /// vector's capacity as it was
        void truncate(std::size_t size) noexcept {
            if (!spilled_.empty()) {
                spilled_.erase(spilled_.begin() + static_cast<std::ptrdiff_t>(size), spilled_.end());
            }
            size_ = size;
        }

        /// Makes room for `size` fields, within the builder while they fit, and
        /// otherwise in the vector, which grows at least twofold, so that
        /// fields added a few at a time take amortized constant time
        TAFLA_ALWAYS_INLINE void reserve(std::size_t size) {
            if (size <= Inline) [[likely]] {
                return;
            }
            grow(size);
        }

        /// Makes room for more fields than the array holds, in the vector
        void grow(std::size_t size) {
            if (spilled_.empty()) {
                // The array holds every field while the vector is empty, so
                // there are at most `Inline`, which the bound shows GCC, whose
                // warnings otherwise see a copy past the array
                const auto inline_fields = static_cast<std::ptrdiff_t>((std::min)(size_, Inline));
                spilled_.reserve((std::max)(size, 2 * Inline));
                spilled_.assign(inline_.begin(), inline_.begin() + inline_fields);
            } else if (size > spilled_.capacity()) {
                spilled_.reserve((std::max)(size, 2 * spilled_.capacity()));
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

    /// What the builder counts of the table whose fields it's recording: its
    /// own, or those of a table that it records in place
    struct Counters {
        std::uint32_t fields;
        std::uint32_t set;
        std::uint32_t length;
        Alignment alignment;
    };

    [[nodiscard]] Counters counters() const noexcept {
        return {fields_, set_, length_, alignment_};
    }

    void restore(const Counters& counters) noexcept {
        fields_ = counters.fields;
        set_ = counters.set;
        length_ = counters.length;
        alignment_ = counters.alignment;
    }

    /// Records a table of `fields` fields in place, whose header, of
    /// `header` bytes, holds only their offsets, and starts counting its
    /// fields rather than this table's
    void start_in_place(std::size_t fields, std::uint32_t header) {
        values_.push_back({Kind::table, 0, Alignment::one, static_cast<std::uint32_t>(fields), 0, 0, nullptr});
        restore({static_cast<std::uint32_t>(fields), 0, header, Alignment::one});
    }

    /// The length of the header, which the constructor checked
    [[nodiscard]] std::uint32_t header_length() const noexcept {
        const auto slots = fields_ + (sized_ ? 1U : 0U) + (versioned_ ? 1U : 0U);
        return static_cast<std::uint32_t>(slots * offset_size(offset_type_));
    }

    /// Leaves the builder as its constructor did, without fields
    void clear() noexcept {
        set_ = 0;
        length_ = header_length();
        alignment_ = Alignment::one;
    }

    /// A field of bytes, whose length the table's length check limits
    static Field bytes_field(Kind kind, const void* data, std::size_t length) noexcept {
        return {kind, 0, Alignment::one, static_cast<std::uint32_t>(length), 0, 0, data};
    }

    /// A field of an array of structs, whose bytes `check_struct_array()`
    /// checked are whole structs of `size` bytes, and whose length the table's
    /// length check limits. Their number, which the table holds, is recorded
    /// rather than their size, which may be wider than 32 bits for no structs.
    static Field struct_array_field(const void* data, std::size_t length, std::size_t size,
                                    Alignment alignment) noexcept {
        return {Kind::struct_array,
                0,
                alignment,
                static_cast<std::uint32_t>(length),
                static_cast<std::uint32_t>(length / size),
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
    TAFLA_ALWAYS_INLINE void set_scalar(T value) {
        const auto length = check_field(sizeof(T));
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
    /// kept, which moving the builder doesn't change. It makes room for the
    /// field that holds the value first, so that recording the field doesn't
    /// throw, and leaves the value and the builder as they were if an
    /// allocation fails.
    template <class T>
    const T& own(T&& value) {
        values_.reserve(values_.size() + 1);
        reserve_owned(1);

        auto owned = std::make_shared<detail::OwnedValue<T>>(std::move(value));
        const T& kept = owned->value;
        owned_.push_back(std::move(owned));
        return kept;
    }

    /// Makes room for `count` more owned values, growing at least twofold,
    /// so that keeping them doesn't throw
    void reserve_owned(std::size_t count) {
        const auto size = owned_.size() + count;
        if (size > owned_.capacity()) {
            owned_.reserve((std::max)(size, 2 * owned_.capacity()));
        }
    }

    /// Removes the fields and owned values recorded past the first `values`
    /// and `owned`, which leaves the builder as it was before a call that
    /// failed partway
    void truncate(std::size_t values, std::size_t owned) noexcept {
        values_.truncate(values);
        owned_.erase(owned_.begin() + static_cast<std::ptrdiff_t>(owned), owned_.end());
    }

    /// The offset size, the width of offsets, counts and lengths
    [[nodiscard]] std::uint64_t width() const noexcept {
        return offset_size(offset_type_);
    }

    /// The table's length with an array of `count` elements of `size` bytes,
    /// padded so that they're aligned to their size
    [[nodiscard]] std::uint32_t check_array(std::size_t count, std::size_t size) const {
        const auto elements = detail::saturating_product(count, size);
        return check_field(detail::array_bytes(length_, width(), elements, static_cast<Alignment>(size)));
    }

    [[nodiscard]] std::uint32_t check_struct_array(std::size_t length, std::size_t size, Alignment alignment) const {
        if (size == 0 || length % size != 0) {
            throw BuildError::not_whole_structs(length, size);
        }

        return check_field(detail::array_bytes(length_, width(), length, alignment));
    }

    /// The table's length with a field of `byte_length` bytes, its padding
    /// included, which checking here, rather than when writing the table,
    /// reports a field too long for the layout from its own setter and leaves
    /// the builder as it was. Every offset, count, length and item end is at
    /// most the table's length, so that's the one value to check. It's
    /// inlined into each setter, with its errors thrown from cold functions,
    /// as is recording the field, which made building the 200 small tables of
    /// the benchmarks' vector of nested tables 22% faster than calls did.
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::uint32_t check_field(std::uint64_t byte_length) const {
        if (set_ == fields_) [[unlikely]] {
            throw_too_many_fields(fields_);
        }

        const auto length = detail::saturating_sum(length_, byte_length);
        if (length > max_value(offset_type_)) [[unlikely]] {
            throw_too_long(set_, length, offset_type_);
        }
        return static_cast<std::uint32_t>(length);
    }

    /// Throw the errors of `check_field()`
    [[noreturn]] TAFLA_COLD static void throw_too_many_fields(std::size_t fields) {
        throw BuildError::too_many_fields(fields);
    }

    [[noreturn]] TAFLA_COLD static void throw_too_long(std::size_t index, std::uint64_t length,
                                                       OffsetType offset_type) {
        throw BuildError::too_long(index, length, offset_type);
    }

    /// Records a field of the table, which makes it `length` bytes long
    TAFLA_ALWAYS_INLINE void commit(const Field& field, std::uint32_t length) {
        values_.push_back(field);
        set(length, field.alignment);
    }

    /// Counts a field of the table, which makes it `length` bytes long and
    /// needs `alignment` from its start
    TAFLA_ALWAYS_INLINE void set(std::uint32_t length, Alignment alignment) noexcept {
        ++set_;
        length_ = length;
        alignment_ = (std::max)(alignment_, alignment);
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
    /// written in place with its fields, whose owned values it takes or
    /// shares, and anything else as its bytes, which it keeps until the table
    /// is written. The caller makes room for the fields, and truncates them
    /// if it throws.
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
    /// The values that the builder owns, which its fields point into. Builders
    /// nested in place share them, so that a builder nested by reference may
    /// be destroyed before its parent is written.
    std::vector<std::shared_ptr<const detail::Owned>> owned_;
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
/// `const` reference or span and owns from an rvalue. A temporary that it
/// can't own, such as a `const Bytes` or a `std::array` returned by value,
/// doesn't convert to one.
///
/// A builder with its parent's offset type and no header is written straight
/// into its parent, which records its fields and takes or shares what it
/// owns. Other builders are written into their own bytes when they're
/// nested, and bytes are copied when the parent is written.
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

    NestedTable(const TableBuilder&& builder) = delete;

    template <detail::Temporary Range>
    NestedTable(Range&& bytes) = delete;
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

/// The fields of a table that a `TableBuilder` records in place, as one of
/// its own fields or an item of a vector, which its `set_table(fields, fill)`
/// and `set_vector(count, fields, fill)` give to the function that sets
/// them. Its setters are the builder's, taking the same values and throwing
/// the same errors, and record each field among the builder's own, so the
/// table needs no builder of its own and its fields are never copied. It's
/// valid only within that function.
class TableBuilder::Item {
public:
    Item(const Item&) = delete;
    Item& operator=(const Item&) = delete;
    ~Item() = default;

    void set_int8(std::int8_t value) {
        builder_->set_int8(value);
    }

    void set_uint8(std::uint8_t value) {
        builder_->set_uint8(value);
    }

    void set_int16(std::int16_t value) {
        builder_->set_int16(value);
    }

    void set_uint16(std::uint16_t value) {
        builder_->set_uint16(value);
    }

    void set_int32(std::int32_t value) {
        builder_->set_int32(value);
    }

    void set_uint32(std::uint32_t value) {
        builder_->set_uint32(value);
    }

    void set_int64(std::int64_t value) {
        builder_->set_int64(value);
    }

    void set_uint64(std::uint64_t value) {
        builder_->set_uint64(value);
    }

    void set_float32(float value) {
        builder_->set_float32(value);
    }

    void set_float64(double value) {
        builder_->set_float64(value);
    }

    void set_int8_array(std::span<const std::int8_t> values) {
        builder_->set_int8_array(values);
    }

    void set_int8_array(std::vector<std::int8_t>&& values) {
        builder_->set_int8_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_int8_array(Values&& values) = delete;

    void set_uint8_array(std::span<const std::uint8_t> values) {
        builder_->set_uint8_array(values);
    }

    void set_uint8_array(std::vector<std::uint8_t>&& values) {
        builder_->set_uint8_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_uint8_array(Values&& values) = delete;

    void set_int16_array(std::span<const std::int16_t> values) {
        builder_->set_int16_array(values);
    }

    void set_int16_array(std::vector<std::int16_t>&& values) {
        builder_->set_int16_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_int16_array(Values&& values) = delete;

    void set_uint16_array(std::span<const std::uint16_t> values) {
        builder_->set_uint16_array(values);
    }

    void set_uint16_array(std::vector<std::uint16_t>&& values) {
        builder_->set_uint16_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_uint16_array(Values&& values) = delete;

    void set_int32_array(std::span<const std::int32_t> values) {
        builder_->set_int32_array(values);
    }

    void set_int32_array(std::vector<std::int32_t>&& values) {
        builder_->set_int32_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_int32_array(Values&& values) = delete;

    void set_uint32_array(std::span<const std::uint32_t> values) {
        builder_->set_uint32_array(values);
    }

    void set_uint32_array(std::vector<std::uint32_t>&& values) {
        builder_->set_uint32_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_uint32_array(Values&& values) = delete;

    void set_int64_array(std::span<const std::int64_t> values) {
        builder_->set_int64_array(values);
    }

    void set_int64_array(std::vector<std::int64_t>&& values) {
        builder_->set_int64_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_int64_array(Values&& values) = delete;

    void set_uint64_array(std::span<const std::uint64_t> values) {
        builder_->set_uint64_array(values);
    }

    void set_uint64_array(std::vector<std::uint64_t>&& values) {
        builder_->set_uint64_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_uint64_array(Values&& values) = delete;

    void set_float32_array(std::span<const float> values) {
        builder_->set_float32_array(values);
    }

    void set_float32_array(std::vector<float>&& values) {
        builder_->set_float32_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_float32_array(Values&& values) = delete;

    void set_float64_array(std::span<const double> values) {
        builder_->set_float64_array(values);
    }

    void set_float64_array(std::vector<double>&& values) {
        builder_->set_float64_array(std::move(values));
    }

    template <detail::Temporary Values>
    void set_float64_array(Values&& values) = delete;

    void set_string(std::string_view string) {
        builder_->set_string(string);
    }

    void set_string(const char* string) {
        builder_->set_string(string);
    }

    void set_string(std::string&& string) {
        builder_->set_string(std::move(string));
    }

    template <detail::Temporary String>
    void set_string(String&& string) = delete;

    void set_struct(std::span<const std::uint8_t> bytes) {
        builder_->set_struct(bytes);
    }

    void set_struct(std::vector<std::uint8_t>&& bytes) {
        builder_->set_struct(std::move(bytes));
    }

    template <detail::Temporary Range>
    void set_struct(Range&& bytes) = delete;

    void set_struct_array(std::span<const std::uint8_t> bytes, std::size_t size, Alignment alignment) {
        builder_->set_struct_array(bytes, size, alignment);
    }

    void set_struct_array(std::vector<std::uint8_t>&& bytes, std::size_t size, Alignment alignment) {
        builder_->set_struct_array(std::move(bytes), size, alignment);
    }

    template <detail::Temporary Range>
    void set_struct_array(Range&& bytes, std::size_t size, Alignment alignment) = delete;

    void set_table(NestedTable table) {
        builder_->set_table(std::move(table));
    }

    void set_table(NestedTable table, Alignment alignment) {
        builder_->set_table(std::move(table), alignment);
    }

    template <std::invocable<Item&> Fill>
    void set_table(std::size_t fields, Fill&& fill) {
        builder_->set_table(fields, std::forward<Fill>(fill));
    }

    void set_vector(std::vector<NestedTable> items) {
        builder_->set_vector(std::move(items));
    }

    template <std::invocable<std::size_t, Item&> Fill>
    void set_vector(std::size_t count, std::size_t fields, Fill&& fill) {
        builder_->set_vector(count, fields, std::forward<Fill>(fill));
    }

private:
    friend class TableBuilder;

    explicit Item(TableBuilder& builder) noexcept : builder_(&builder) {}

    TableBuilder* builder_;
};

template <std::invocable<TableBuilder::Item&> Fill>
void TableBuilder::set_table(std::size_t fields, Fill&& fill) {
    if (set_ == fields_) [[unlikely]] {
        throw_too_many_fields(fields_);
    }
    const auto header = detail::header_length(fields, false, false, offset_type_);

    const auto values = values_.size();
    const auto owned = owned_.size();
    const auto parent = counters();
    try {
        start_in_place(fields, header);
        Item item(*this);
        std::invoke(fill, item);
        check_complete();

        // Padded from this table's length, as a nested builder is
        const auto table = counters();
        restore(parent);
        const std::uint64_t padding = detail::padding<std::uint64_t>(length_, table.alignment);
        const auto length = check_field(padding + table.length);
        values_.set_alignment(values, table.alignment);
        set(length, table.alignment);
    } catch (...) {
        restore(parent);
        truncate(values, owned);
        throw;
    }
}

template <std::invocable<std::size_t, TableBuilder::Item&> Fill>
void TableBuilder::set_vector(std::size_t count, std::size_t fields, Fill&& fill) {
    // The count and the items' ends, which fail before any item is recorded
    const auto slots = detail::vector_bytes(count, width());
    static_cast<void>(check_field(slots));
    const auto header = detail::header_length(fields, false, false, offset_type_);

    const auto values = values_.size();
    const auto owned = owned_.size();
    const auto parent = counters();
    try {
        // Room for the fewest fields that the items record, one for each
        // table and each of its fields, which is all of them unless they
        // nest tables of their own, so that recording them grows the fields
        // at most a few times. It never reserves more than they record.
        const auto fewest = detail::saturating_product(count, detail::saturating_sum(fields, 1));
        if (fewest < SIZE_MAX - values) {
            values_.reserve(values + 1 + static_cast<std::size_t>(fewest));
        }
        values_.push_back({Kind::vector, 0, Alignment::one, static_cast<std::uint32_t>(count), 0, 0, nullptr});

        std::uint64_t nested_bytes = 0;
        for (std::size_t index = 0; index < count; ++index) {
            start_in_place(fields, header);
            Item item(*this);
            std::invoke(fill, index, item);
            check_complete();

            nested_bytes = detail::saturating_sum(nested_bytes, length_);
            restore(parent);
        }

        const auto length = check_field(detail::saturating_sum(slots, nested_bytes));
        set(length, Alignment::one);
    } catch (...) {
        restore(parent);
        truncate(values, owned);
        throw;
    }
}

inline TableBuilder::TableBuilder(std::size_t fields, std::uint32_t header, bool sized,
                                  std::optional<std::uint32_t> version, OffsetType offset_type)
    // The header holds an offset of at least a byte per field
    : fields_(static_cast<std::uint32_t>(fields)),
      length_(header),
      version_(version.value_or(0)),
      versioned_(version.has_value()),
      sized_(sized),
      offset_type_(offset_type) {
    // Room for the fields of a table of up to a few hundred, which a table
    // of more grows from as they're set, rather than for every field that a
    // header can hold, up to 2^30, whatever is set
    constexpr std::size_t reserved = 256;
    values_.reserve((std::min)(fields, reserved));
}

inline TableBuilder::TableBuilder(TableBuilder&& other) noexcept
    : values_(std::move(other.values_)),
      owned_(std::move(other.owned_)),
      fields_(other.fields_),
      set_(other.set_),
      length_(other.length_),
      version_(other.version_),
      versioned_(other.versioned_),
      sized_(other.sized_),
      offset_type_(other.offset_type_),
      alignment_(other.alignment_) {
    other.owned_.clear();
    other.clear();
}

inline TableBuilder& TableBuilder::operator=(TableBuilder&& other) noexcept {
    if (this != &other) {
        values_ = std::move(other.values_);
        owned_ = std::move(other.owned_);
        fields_ = other.fields_;
        set_ = other.set_;
        length_ = other.length_;
        version_ = other.version_;
        versioned_ = other.versioned_;
        sized_ = other.sized_;
        offset_type_ = other.offset_type_;
        alignment_ = other.alignment_;
        other.owned_.clear();
        other.clear();
    }
    return *this;
}

inline TableBuilder::~TableBuilder() = default;

inline void TableBuilder::set_table(NestedTable table) {
    const auto alignment = table.default_alignment();
    set_table(std::move(table), alignment);
}

inline void TableBuilder::set_table(NestedTable table, Alignment alignment) {
    table.check_complete();

    // Padded from the table's length, unlike an array, since it has no count
    const std::uint64_t padding = detail::padding<std::uint64_t>(length_, alignment);
    const auto length = check_field(padding + table.byte_length());

    values_.reserve(values_.size() + table.recorded_fields(offset_type_));
    append_nested(std::move(table), alignment);
    set(length, alignment);
}

inline void TableBuilder::set_vector(std::vector<NestedTable> items) {
    std::uint64_t nested_bytes = 0;
    std::size_t recorded = 1;
    for (const auto& item : items) {
        item.check_complete();
        nested_bytes = detail::saturating_sum(nested_bytes, item.byte_length());
        recorded += item.recorded_fields(offset_type_);
    }

    const auto slots = detail::vector_bytes(items.size(), width());
    const auto length = check_field(detail::saturating_sum(slots, nested_bytes));

    // Each item may allocate, or write a builder of another layout, so the
    // items recorded before one that fails are removed
    const auto values = values_.size();
    const auto owned = owned_.size();
    values_.reserve(values + recorded);
    values_.push_back({Kind::vector, 0, Alignment::one, static_cast<std::uint32_t>(items.size()), 0, 0, nullptr});
    try {
        for (auto& item : items) {
            append_nested(std::move(item), Alignment::one);
        }
    } catch (...) {
        truncate(values, owned);
        throw;
    }
    set(length, Alignment::one);
}

inline void TableBuilder::append_nested(NestedTable&& table, Alignment alignment) {
    // Room for what a builder in place owns comes first, so that recording
    // its fields and what it owns doesn't throw. Its fields point into what
    // it owns, which this builder then keeps alive.
    if (auto* owned = std::get_if<TableBuilder>(&table.table_); owned != nullptr && owned->in_place_in(offset_type_)) {
        reserve_owned(owned->owned_.size());
        values_.push_back({Kind::table, 0, alignment, owned->fields_, 0, 0, nullptr});
        values_.append(owned->values_);
        std::move(owned->owned_.begin(), owned->owned_.end(), std::back_inserter(owned_));
        return;
    }

    if (const auto* const* borrowed = std::get_if<const TableBuilder*>(&table.table_);
        borrowed != nullptr && (*borrowed)->in_place_in(offset_type_)) {
        reserve_owned((*borrowed)->owned_.size());
        values_.push_back({Kind::table, 0, alignment, (*borrowed)->fields_, 0, 0, nullptr});
        values_.append((*borrowed)->values_);
        owned_.insert(owned_.end(), (*borrowed)->owned_.begin(), (*borrowed)->owned_.end());
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
                writer.start_array(index, field.structs, field.alignment);
                writer.push(field.data, field.count);
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
