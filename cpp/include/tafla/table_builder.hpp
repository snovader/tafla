// Builds a table from its fields, set one after another.

#ifndef TAFLA_TABLE_BUILDER_HPP
#define TAFLA_TABLE_BUILDER_HPP

#include <algorithm>
#include <array>
#include <concepts>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <functional>
#include <iterator>
#include <memory>
#include <new>
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

/// A value that a builder owns until it's destroyed, whatever its type
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

/// The bytes that a builder writes its table into: the first `Inline` within
/// the builder, and then on the heap, where they grow at least twofold, and
/// to at least `FirstHeap`, keeping the bytes written.
///
/// The heap's bytes are new bytes, which the old ones are copied into, rather
/// than bytes that `std::realloc()` extends where it can: a prototype that
/// grew them so was no faster in the comparison's workloads, since the runs
/// that a builder keeps rather than copies leave it few bytes to grow.
class GrowingBytes {
public:
    static constexpr std::size_t Inline = 128;
    /// The least that the bytes grow to on the heap, so that a table that
    /// outgrows the builder's own bytes doesn't grow again soon
    static constexpr std::size_t FirstHeap = 512;

    GrowingBytes() noexcept = default;

    GrowingBytes(GrowingBytes&& other) noexcept {
        take(other);
    }

    GrowingBytes& operator=(GrowingBytes&& other) noexcept {
        if (this != &other) {
            release();
            take(other);
        }
        return *this;
    }

    GrowingBytes(const GrowingBytes&) = delete;
    GrowingBytes& operator=(const GrowingBytes&) = delete;

    ~GrowingBytes() {
        release();
    }

    [[nodiscard]] std::uint8_t* data() noexcept {
        return data_;
    }

    [[nodiscard]] const std::uint8_t* data() const noexcept {
        return data_;
    }

    [[nodiscard]] std::size_t size() const noexcept {
        return size_;
    }

    /// Adds `count` bytes, which the caller writes, and returns where they
    /// start. Leaves the bytes as they were if growing them fails.
    TAFLA_ALWAYS_INLINE std::uint8_t* extend(std::size_t count) {
        if (count > capacity_ - size_) [[unlikely]] {
            grow(count);
        }
        std::uint8_t* at = data_ + size_;
        size_ += count;
        return at;
    }

    /// Makes room for `count` more bytes, so that adding them doesn't grow
    /// the bytes
    void reserve_more(std::size_t count) {
        if (count > capacity_ - size_) {
            grow(count);
        }
    }

    /// Removes the bytes past the first `size`
    void truncate(std::size_t size) noexcept {
        size_ = size;
    }

private:
    TAFLA_COLD void grow(std::size_t count) {
        if (count > SIZE_MAX / 2 - size_) {
            throw std::bad_alloc();
        }

        const std::size_t capacity = (std::max)({size_ + count, 2 * capacity_, FirstHeap});
        auto* grown = static_cast<std::uint8_t*>(::operator new(capacity));
        copy(grown, data_, size_);
        release();
        data_ = grown;
        capacity_ = capacity;
    }

    [[nodiscard]] bool is_inline() const noexcept {
        return data_ == inline_.data();
    }

    void release() noexcept {
        if (!is_inline()) {
            ::operator delete(data_);
        }
    }

    /// Takes `other`'s bytes, and leaves it with none, within itself
    void take(GrowingBytes& other) noexcept {
        size_ = other.size_;
        if (other.is_inline()) {
            copy(inline_.data(), other.inline_.data(), other.size_);
            data_ = inline_.data();
            capacity_ = Inline;
        } else {
            data_ = other.data_;
            capacity_ = other.capacity_;
        }

        other.data_ = other.inline_.data();
        other.size_ = 0;
        other.capacity_ = Inline;
    }

    // Not initialized: a builder writes every byte that it adds
    alignas(8) std::array<std::uint8_t, Inline> inline_;
    std::uint8_t* data_ = inline_.data();
    std::size_t size_ = 0;
    std::size_t capacity_ = Inline;
};

/// A run of bytes that a builder borrows or owns rather than copying, which
/// lies in its table before the builder's own byte at `at`
struct Segment {
    std::size_t at;
    const std::uint8_t* data;
    std::size_t size;
};

/// The segments of a table, in the order of their positions: the first
/// `Inline` within the builder, and the others in a vector, which holds them
/// all once they don't fit
class Segments {
public:
    static constexpr std::size_t Inline = 4;

    Segments() = default;

    /// Leaves `other` without segments, and without a vector, whose room
    /// would otherwise say that it holds them
    Segments(Segments&& other) noexcept
        : inline_(other.inline_), spilled_(std::move(other.spilled_)), size_(std::exchange(other.size_, 0)) {
        other.spilled_ = {};
    }

    Segments& operator=(Segments&& other) noexcept {
        if (this != &other) {
            inline_ = other.inline_;
            spilled_ = std::move(other.spilled_);
            size_ = std::exchange(other.size_, 0);
            other.spilled_ = {};
        }
        return *this;
    }

    Segments(const Segments&) = delete;
    Segments& operator=(const Segments&) = delete;
    ~Segments() = default;

    [[nodiscard]] std::size_t size() const noexcept {
        return size_;
    }

    [[nodiscard]] Segment* begin() noexcept {
        return spilled() ? spilled_.data() : inline_.data();
    }

    [[nodiscard]] const Segment* begin() const noexcept {
        return spilled() ? spilled_.data() : inline_.data();
    }

    [[nodiscard]] const Segment* end() const noexcept {
        return begin() + size_;
    }

    [[nodiscard]] Segment& operator[](std::size_t index) noexcept {
        return begin()[index];
    }

    /// Makes room for `count` more segments, so that adding them doesn't
    /// throw
    void reserve_more(std::size_t count) {
        const auto size = size_ + count;
        if (!spilled()) {
            if (size <= Inline) {
                return;
            }
            std::vector<Segment> spilled;
            spilled.reserve((std::max)(size, 2 * Inline));
            spilled.assign(inline_.begin(), inline_.begin() + static_cast<std::ptrdiff_t>((std::min)(size_, Inline)));
            spilled_ = std::move(spilled);
        } else if (size > spilled_.capacity()) {
            spilled_.reserve((std::max)(size, 2 * spilled_.capacity()));
        }
    }

    /// Adds a segment, for which there's room
    void push_back(const Segment& segment) noexcept {
        if (spilled()) {
            spilled_.push_back(segment);
        } else {
            inline_[size_] = segment;
        }
        ++size_;
    }

    /// Removes the segments past the first `size`, keeping the vector's room
    void truncate(std::size_t size) noexcept {
        if (spilled()) {
            spilled_.resize(size);
        }
        size_ = size;
    }

    void clear() noexcept {
        spilled_.clear();
        size_ = 0;
    }

private:
    /// Whether the vector holds the segments, which it does once it has
    /// room, even for none
    [[nodiscard]] bool spilled() const noexcept {
        return spilled_.capacity() != 0;
    }

    // Not initialized: only the first `size_` are read. Zeroing them made
    // building the comparison's record 2 ns slower.
    std::array<Segment, Inline> inline_;
    std::vector<Segment> spilled_;
    std::size_t size_ = 0;
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
/// The builder writes the table forward as its fields are set: the header
/// when it's started, and then each field's bytes after the last, and its
/// offset into the header. It writes into bytes that it holds, the first 128
/// within itself and the others on the heap, growing at least twofold.
/// `to_bytes()` copies them into bytes of the table's exact length, and
/// writes a sized table's length there. Runs of 256 bytes or more, the
/// elements of an array, a string's bytes or a nested table's, aren't copied
/// when they're set: the builder keeps them where they are, and `to_bytes()`
/// copies them into the table between its own bytes, so that a long array is
/// copied once.
///
/// A setter that takes a `std::span` or a `std::string_view` borrows its
/// value, which must outlive the builder's last `to_bytes()`, as a view
/// would. One that takes a `std::vector`, a `std::string`, a `Bytes` or a
/// builder by rvalue reference owns it. A call with a temporary of another
/// container, such as a `std::array` returned by value, doesn't compile,
/// since the builder would view it after it's destroyed. A builder nested by
/// `const` reference is copied into this one when it's nested, as
/// `set_table()` describes, so it needn't outlive this one.
///
/// A setter throws a `BuildError`, and leaves the builder as it was, when the
/// table already holds all its fields or the field would make it too long
/// for the layout's offset type: longer than 255 bytes, 64 KiB or 4 GiB.
class TableBuilder {
public:
    /// Starts a table of `fields` fields in `layout`, which its readers must
    /// use too, writing its header: an offset for each field, and the slots
    /// of the layout's length and version. Nested tables use the default
    /// layout.
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

    /// Moves `other`'s bytes and what it owns, and leaves it as a builder
    /// that was just started, with its layout and number of fields, which
    /// throws a `BuildError` until every field is set again. It writes its
    /// header again when its first field is set, so that moving it allocates
    /// nothing.
    TableBuilder(TableBuilder&& other) noexcept;
    TableBuilder& operator=(TableBuilder&& other) noexcept;
    TableBuilder(const TableBuilder&) = delete;
    TableBuilder& operator=(const TableBuilder&) = delete;
    ~TableBuilder() = default;

    /// Writes the table, at its exact size.
    ///
    /// Throws a `BuildError` if fewer fields were set than the table holds.
    [[nodiscard]] Bytes to_bytes() const {
        check_complete();

        auto bytes = Bytes::for_overwrite(position());
        copy_into(bytes.data());
        if (sized_) {
            detail::store_slot(bytes.data(), position(), offset_type_);
        }
        return bytes;
    }

    /// The table's byte length with the fields set so far
    [[nodiscard]] std::size_t byte_length() const noexcept {
        return restarting_ ? header_length(restart_fields_) : position();
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
    TAFLA_ALWAYS_INLINE void set_int8(std::int8_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint8(std::uint8_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_int16(std::int16_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint16(std::uint16_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_int32(std::int32_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint32(std::uint32_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_int64(std::int64_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint64(std::uint64_t value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_float32(float value) {
        set_scalar(value);
    }

    TAFLA_ALWAYS_INLINE void set_float64(double value) {
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
        check_field(detail::saturating_sum(width(), string.size()));
        put_counted(0, string.size(), string.data(), string.size(), Alignment::one, nullptr);
    }

    void set_string(const char* string) {
        set_string(std::string_view(string));
    }

    void set_string(std::string&& string) {
        check_field(detail::saturating_sum(width(), string.size()));
        const auto kept = keep(std::move(string));
        put_counted(0, kept.value.size(), kept.value.data(), kept.value.size(), Alignment::one, kept.owner);
    }

    template <detail::Temporary String>
    void set_string(String&& string) = delete;

    /// Sets a struct field from its bytes: its scalars, little-endian, one
    /// after another, which are written as they are, without padding.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_struct(std::span<const std::uint8_t> bytes) {
        check_field(bytes.size());
        put_struct(bytes.data(), bytes.size(), nullptr);
    }

    void set_struct(std::vector<std::uint8_t>&& bytes) {
        check_field(bytes.size());
        const auto kept = keep(std::move(bytes));
        put_struct(kept.value.data(), kept.value.size(), kept.owner);
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
        const auto padding = check_struct_array(bytes.size(), size, alignment);
        put_counted(padding, bytes.size() / size, bytes.data(), bytes.size(), alignment, nullptr);
    }

    void set_struct_array(std::vector<std::uint8_t>&& bytes, std::size_t size, Alignment alignment) {
        const auto padding = check_struct_array(bytes.size(), size, alignment);
        const auto kept = keep(std::move(bytes));
        put_counted(padding, kept.value.size() / size, kept.value.data(), kept.value.size(), alignment, kept.owner);
    }

    template <detail::Temporary Range>
    void set_struct_array(Range&& bytes, std::size_t size, Alignment alignment) = delete;

    /// Embeds a table, from its builder or its bytes, without a length
    /// prefix. Readers find its fields with `ByteBuffer::field_offset()` from
    /// the field's position, which reads offsets of the parent's offset type,
    /// so the table should have no header and the parent's offset type:
    /// that's the default layout in a parent of `uint32` offsets.
    ///
    /// A builder must have all its fields set, and is copied into this table
    /// at once, in its own layout, as its bytes and the runs that it keeps
    /// rather than copies, whose owners this builder takes, from an rvalue,
    /// or shares, from a `const` reference. The nested builder may then
    /// change or be destroyed; only the values that it borrows must outlive
    /// this builder's last `to_bytes()`.
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
    /// others. Builders must have all their fields set, and are copied into
    /// this table as `set_table()` copies them. Items aren't padded, so their
    /// arrays are read in place only when they happen to be aligned.
    ///
    /// Throws a `BuildError` if a builder has fields left to set, or as the
    /// scalars' setters do, and leaves the builder as it was, as it does if
    /// an allocation fails.
    void set_vector(std::vector<NestedTable> items);

    class Item;

    /// Writes a table of `fields` fields nested in this one, in place, as a
    /// nested builder would be, but without a builder of its own: `fill` sets
    /// its fields, in order, on the `Item` that it's given, and each is
    /// written straight into this builder's bytes. The table is padded to
    /// start at a multiple of its largest alignment, as `set_table()` pads a
    /// builder: it's written where this table's next field goes, and moved
    /// forward by its padding once its fields are set, which changes none of
    /// its offsets, since they count from its own start.
    ///
    /// The builder itself mustn't be used within `fill`, only the `Item`.
    ///
    /// Throws a `BuildError` if `fill` leaves fields of the table unset, or as
    /// the setters do, and passes on what `fill` throws, leaving the builder
    /// as it was either way.
    template <std::invocable<Item&> Fill>
    void set_table(std::size_t fields, Fill&& fill);

    /// Writes a vector of `count` tables of `fields` fields each, in place,
    /// as `set_table(fields, fill)` writes one: `fill(index, item)` sets the
    /// fields of item `index`, for each in order. Items aren't padded, as
    /// `set_vector()` doesn't pad them.
    ///
    /// Throws a `BuildError` if the vector's count makes the table too long
    /// before any item is written, if `fill` leaves an item's fields unset,
    /// or as the setters do, and passes on what `fill` throws, leaving the
    /// builder as it was either way.
    template <std::invocable<std::size_t, Item&> Fill>
    void set_vector(std::size_t count, std::size_t fields, Fill&& fill);

private:
    friend class NestedTable;

    /// Runs of at least this many bytes, an array's elements, a string's or a
    /// nested table's bytes, borrowed or owned, are kept as segments of the
    /// table, which `to_bytes()` copies, rather than copied into the
    /// builder's bytes when they're set. Copying every run when it's set made
    /// building the comparison's document of 100,000 rows 3.8 times slower,
    /// since each 400 KB column was copied into the bytes, which grew to
    /// hold them, and then into the table.
    static constexpr std::size_t kSegmentBytes = 256;

    using Owner = std::shared_ptr<const detail::Owned>;

    /// A value that a setter was given to own, and the owner that the builder
    /// keeps it in, or none for a value too short to keep, which is copied
    template <class T>
    struct Kept {
        const T& value;
        const Owner* owner;
    };

    /// What the builder counts of the table that it's writing: its own, or
    /// one that it writes in place
    struct Counters {
        std::size_t table;
        std::size_t slots;
        std::uint32_t fields;
        std::uint32_t set;
        Alignment alignment;
    };

    /// What the builder holds, which a nested table or vector whose `fill`
    /// throws is truncated back to
    struct Checkpoint {
        Counters counters;
        std::size_t bytes;
        std::size_t shift;
        std::size_t segments;
        std::size_t owned;
    };

    TableBuilder(std::size_t fields, std::uint32_t header, bool sized, std::optional<std::uint32_t> version,
                 OffsetType offset_type)
        // The header holds an offset of at least a byte per field
        : fields_(static_cast<std::uint32_t>(fields)),
          version_(version.value_or(0)),
          versioned_(version.has_value()),
          sized_(sized),
          offset_type_(offset_type) {
        write_header(header);
    }

    static std::uint32_t versioned_header(std::size_t fields, TableLayout<true> layout, std::uint32_t version) {
        detail::check_version(version, layout.offset_type());
        return detail::header_length(fields, layout.is_sized(), true, layout.offset_type());
    }

    /// The offset size, the width of offsets, counts and lengths
    [[nodiscard]] std::size_t width() const noexcept {
        return offset_size(offset_type_);
    }

    /// The length of the header of the builder's table with `fields` fields,
    /// which the constructor checked
    [[nodiscard]] std::uint32_t header_length(std::uint32_t fields) const noexcept {
        const auto slots = fields + (sized_ ? 1U : 0U) + (versioned_ ? 1U : 0U);
        return static_cast<std::uint32_t>(slots * width());
    }

    /// Writes the header of `header` bytes: its length's slot, which
    /// `to_bytes()` writes, its version and its offsets, written as each
    /// field is set
    void write_header(std::uint32_t header) {
        auto* at = bytes_.extend(header);
        detail::zero(at, header);

        if (versioned_) {
            detail::store_slot(at + (sized_ ? width() : 0), version_, offset_type_);
        }
        slots_ = ((sized_ ? 1U : 0U) + (versioned_ ? 1U : 0U)) * width();
    }

    /// Leaves the builder as its constructor did, without fields, after its
    /// bytes were moved. Its header, unless it's only the length's and
    /// version's slots, is written again when the first field is set, so
    /// that moving the builder doesn't allocate.
    void restart() noexcept {
        const auto fields = restarting_ ? restart_fields_ : fields_;
        segments_.clear();
        owned_.clear();
        shift_ = 0;
        table_ = 0;
        set_ = 0;
        alignment_ = Alignment::one;

        if (fields == 0) {
            // Within the builder's own bytes, which hold two slots
            write_header(header_length(0));
            fields_ = 0;
            restarting_ = false;
        } else {
            fields_ = 0;
            restart_fields_ = fields;
            restarting_ = true;
        }
    }

    /// The position in the table of the next byte: the bytes written and
    /// the segments before them
    [[nodiscard]] std::size_t position() const noexcept {
        return bytes_.size() + shift_;
    }

    /// The length of the table being written, which is where its next field
    /// goes
    [[nodiscard]] std::size_t table_length() const noexcept {
        return position() - table_;
    }

    [[nodiscard]] Counters counters() const noexcept {
        return {table_, slots_, fields_, set_, alignment_};
    }

    void restore(const Counters& counters) noexcept {
        table_ = counters.table;
        slots_ = counters.slots;
        fields_ = counters.fields;
        set_ = counters.set;
        alignment_ = counters.alignment;
    }

    [[nodiscard]] Checkpoint checkpoint() const noexcept {
        return {counters(), bytes_.size(), shift_, segments_.size(), owned_.size()};
    }

    void truncate(const Checkpoint& checkpoint) noexcept {
        restore(checkpoint.counters);
        bytes_.truncate(checkpoint.bytes);
        shift_ = checkpoint.shift;
        segments_.truncate(checkpoint.segments);
        owned_.erase(owned_.begin() + static_cast<std::ptrdiff_t>(checkpoint.owned), owned_.end());
    }

    /// Checks that the builder's table has every field set, for it to be
    /// written or nested
    void check_complete() const {
        if (restarting_) {
            throw BuildError::incomplete(0, restart_fields_);
        }
        check_filled();
    }

    /// Checks that the table being written, the builder's or one in place,
    /// has every field set
    void check_filled() const {
        if (set_ != fields_) {
            throw BuildError::incomplete(set_, fields_);
        }
    }

    /// Checks that the table has a field left to set, and that a field of
    /// `byte_length` bytes, which don't depend on where it starts, leaves it
    /// short enough. Every offset, count, length and item end is at most the
    /// table's length, so that's the one value to check. It's inlined into
    /// each setter, with its errors thrown from cold functions.
    TAFLA_ALWAYS_INLINE void check_field(std::uint64_t byte_length) {
        check_room();
        check_length(detail::saturating_sum(table_length(), byte_length));
    }

    /// Checks that the table has a field left to set
    TAFLA_ALWAYS_INLINE void check_room() {
        if (set_ == fields_) [[unlikely]] {
            restart_or_throw();
        }
    }

    /// Checks that the table is short enough at `length`, after its field
    /// `set_`
    TAFLA_ALWAYS_INLINE void check_length(std::uint64_t length) const {
        if (length > max_value(offset_type_)) [[unlikely]] {
            throw_too_long(set_, length, offset_type_);
        }
    }

    /// Throws the error of a field past the table's last, unless the builder
    /// was moved from: then it writes the header again, and its fields are
    /// set again, from the first
    TAFLA_COLD void restart_or_throw() {
        if (!restarting_) {
            throw BuildError::too_many_fields(fields_);
        }

        write_header(header_length(restart_fields_));
        fields_ = restart_fields_;
        restarting_ = false;
    }

    [[noreturn]] TAFLA_COLD static void throw_too_long(std::size_t index, std::uint64_t length,
                                                       OffsetType offset_type) {
        throw BuildError::too_long(index, length, offset_type);
    }

    /// Checks an array of structs, and returns the padding before its count
    [[nodiscard]] std::size_t check_struct_array(std::size_t length, std::size_t size, Alignment alignment) {
        if (size == 0 || length % size != 0) {
            throw BuildError::not_whole_structs(length, size);
        }
        return check_array(length, alignment);
    }

    /// Checks an array field of `byte_length` bytes of elements, aligned to
    /// `alignment`, and returns the padding before its count, which depends
    /// on the table's length, and so follows the check of its room, which
    /// writes the header of a builder that was moved from
    [[nodiscard]] TAFLA_ALWAYS_INLINE std::size_t check_array(std::uint64_t byte_length, Alignment alignment) {
        check_room();

        const auto length = table_length();
        check_length(detail::saturating_sum(length, detail::array_bytes(length, width(), byte_length, alignment)));
        return detail::padding(length + width(), alignment);
    }

    /// Writes the offset of the field being set, which starts at `position`
    TAFLA_ALWAYS_INLINE void write_offset(std::size_t position) noexcept {
        detail::store_slot(bytes_.data() + slots_ + std::size_t{set_} * width(), position - table_, offset_type_);
    }

    /// Counts the field just written, which needs `alignment` from the
    /// table's start
    TAFLA_ALWAYS_INLINE void commit(Alignment alignment) noexcept {
        ++set_;
        alignment_ = (std::max)(alignment_, alignment);
    }

    /// Whether a run of `size` bytes is kept as a segment rather than copied
    [[nodiscard]] static constexpr bool is_segment(std::size_t size) noexcept {
        return size >= kSegmentBytes;
    }

    /// Makes room for `count` more owned values, growing at least twofold,
    /// so that keeping them doesn't throw
    void reserve_owned(std::size_t count) {
        const auto size = owned_.size() + count;
        if (size > owned_.capacity()) {
            owned_.reserve((std::max<std::size_t>)({size, 2 * owned_.capacity(), 4}));
        }
    }

    /// Keeps `value` until the builder is destroyed, if it's long enough to
    /// be a segment, so that its bytes needn't be copied when it's set. A
    /// shorter value is copied, and left to the caller.
    template <class T>
    Kept<T> keep(T&& value) {
        const auto size = value.size() * sizeof(typename T::value_type);
        if (!is_segment(size)) {
            return {value, nullptr};
        }

        reserve_owned(1);
        auto owned = std::make_shared<detail::OwnedValue<T>>(std::move(value));
        const T& kept = owned->value;
        owned_.push_back(std::move(owned));
        return {kept, &owned_.back()};
    }

    /// Writes a struct field of `size` bytes from `data`, which has no
    /// padding, and its offset. The setter checked it, and kept its bytes if
    /// they're owned, which this removes if growing the bytes fails.
    void put_struct(const void* data, std::size_t size, const Owner* owner) {
        const auto start = position();
        if (is_segment(size)) {
            reserve_or_forget(owner, [&] { segments_.reserve_more(1); });
            write_offset(start);
            add_segment(data, size);
        } else {
            auto* at = reserve_or_forget(owner, [&] { return bytes_.extend(size); });
            write_offset(start);
            detail::copy(at, data, size);
        }
        commit(Alignment::one);
    }

    /// Writes a field of a count, `count`, and then `size` bytes from `data`,
    /// after `padding` zeros, as `put_struct()` writes a struct
    void put_counted(std::size_t padding, std::size_t count, const void* data, std::size_t size, Alignment alignment,
                     const Owner* owner) {
        const auto start = position() + padding;
        std::uint8_t* at = nullptr;
        if (is_segment(size)) {
            at = reserve_or_forget(owner, [&] {
                segments_.reserve_more(1);
                return bytes_.extend(padding + width());
            });
        } else {
            at = reserve_or_forget(owner, [&] { return bytes_.extend(padding + width() + size); });
        }

        detail::zero(at, padding);
        write_offset(start);
        detail::store_slot(at + padding, count, offset_type_);
        if (is_segment(size)) {
            add_segment(data, size);
        } else {
            detail::copy(at + padding + width(), data, size);
        }
        commit(alignment);
    }

    /// Calls `reserve`, which makes room for a field, and removes the owned
    /// value of the field if it throws
    template <class Reserve>
    TAFLA_ALWAYS_INLINE auto reserve_or_forget(const Owner* owner, Reserve reserve) -> decltype(reserve()) {
        if (owner == nullptr) {
            return reserve();
        }
        try {
            return reserve();
        } catch (...) {
            owned_.pop_back();
            throw;
        }
    }

    /// Adds a segment of `size` bytes at `data`, for which there's room,
    /// before the next byte written
    void add_segment(const void* data, std::size_t size) noexcept {
        segments_.push_back({bytes_.size(), static_cast<const std::uint8_t*>(data), size});
        shift_ += size;
    }

    template <class T>
    TAFLA_ALWAYS_INLINE void set_scalar(T value) {
        check_field(sizeof(T));

        const auto start = position();
        auto* at = bytes_.extend(sizeof(T));
        write_offset(start);
        detail::store(at, value);
        commit(Alignment::one);
    }

    template <class T>
    void set_array(std::span<const T> values) {
        constexpr auto alignment = static_cast<Alignment>(sizeof(T));
        const auto padding = check_array(detail::saturating_product(values.size(), sizeof(T)), alignment);
        if constexpr (detail::kHostOrder || sizeof(T) == 1) {
            put_counted(padding, values.size(), values.data(), values.size_bytes(), alignment, nullptr);
        } else {
            put_swapped(padding, values);
        }
    }

    template <class T>
    void set_owned_array(std::vector<T>&& values) {
        if constexpr (detail::kHostOrder || sizeof(T) == 1) {
            constexpr auto alignment = static_cast<Alignment>(sizeof(T));
            const auto padding = check_array(detail::saturating_product(values.size(), sizeof(T)), alignment);
            const auto kept = keep(std::move(values));
            put_counted(padding, kept.value.size(), kept.value.data(), kept.value.size() * sizeof(T), alignment,
                        kept.owner);
        } else {
            // Swapped as they're copied, so they needn't be kept
            set_array(std::span<const T>(values));
        }
    }

    /// Writes an array whose elements are swapped to little-endian one by
    /// one, on a big-endian host
    template <class T>
    void put_swapped(std::size_t padding, std::span<const T> values) {
        const auto start = position() + padding;
        auto* at = bytes_.extend(padding + width() + values.size_bytes());
        detail::zero(at, padding);
        write_offset(start);
        detail::store_slot(at + padding, values.size(), offset_type_);

        auto* element = at + padding + width();
        for (const T value : values) {
            detail::store(element, value);
            element += sizeof(T);
        }
        commit(static_cast<Alignment>(sizeof(T)));
    }

    /// The bytes, segments and owned values that appending nested tables
    /// adds, for which `append_nested()` needs room
    struct Room {
        std::size_t bytes = 0;
        std::size_t segments = 0;
        std::size_t owned = 0;
    };

    /// Adds the room that appending `table` needs to `room`
    static void add_room(Room& room, const NestedTable& table) noexcept;

    void reserve(const Room& room) {
        segments_.reserve_more(room.segments);
        reserve_owned(room.owned);
        bytes_.reserve_more(room.bytes);
    }

    /// Appends a nested table's bytes, for which `reserve()` made room: a
    /// builder's bytes and segments, with its length in a sized header, or
    /// bytes, kept as a segment when they're long enough. It takes what a
    /// builder owns from an rvalue, and shares it from a `const` reference.
    /// Only keeping owned bytes allocates, which the caller truncates after.
    void append_nested(NestedTable& table);

    /// Appends the bytes and segments of a complete builder
    void append_builder(const TableBuilder& builder) noexcept {
        const auto start = bytes_.size();
        const auto size = builder.bytes_.size();
        auto* at = bytes_.extend(size);
        detail::copy(at, builder.bytes_.data(), size);
        if (builder.sized_) {
            detail::store_slot(at, builder.position(), builder.offset_type_);
        }

        for (const auto& segment : builder.segments_) {
            segments_.push_back({start + segment.at, segment.data, segment.size});
        }
        shift_ += builder.shift_;
    }

    /// Makes room, after the first of a vector's `count` items wrote `first`
    /// bytes, for the others: as many bytes each as the first's, so that
    /// items alike, as a vector's usually are, grow the bytes at most once,
    /// but for no more than the table's offsets address. A first item much
    /// longer than the others leaves room unused until the builder is
    /// destroyed.
    void reserve_items_like(std::size_t first, std::size_t count) {
        const auto like_first = detail::saturating_product(count - 1, first);
        bytes_.reserve_more(static_cast<std::size_t>((std::min<std::uint64_t>)(like_first, max_value(offset_type_))));
    }

    /// Starts a table of `fields` fields in place, whose header of `header`
    /// bytes holds only their offsets, at the next byte, and counts its
    /// fields rather than its parent's
    TAFLA_ALWAYS_INLINE void start_in_place(std::uint32_t fields, std::uint32_t header) {
        const auto start = position();
        auto* at = bytes_.extend(header);
        detail::zero(at, header);
        restore({start, bytes_.size() - header, fields, 0, Alignment::one});
    }

    /// Moves the table written in place since `saved` forward by `padding`,
    /// with its segments, and zeros the padding before it. None of the
    /// comparison's tables needs it, so it's called rather than inlined.
    TAFLA_COLD void move_by(std::size_t padding, const Checkpoint& saved) {
        const auto length = bytes_.size() - saved.bytes;
        bytes_.extend(padding);
        auto* table = bytes_.data() + saved.bytes;
        std::memmove(table + padding, table, length);
        detail::zero(table, padding);
        for (auto segment = saved.segments; segment < segments_.size(); ++segment) {
            segments_[segment].at += padding;
        }
    }

    /// Copies the table's bytes, with its segments in their places, to `to`
    void copy_into(std::uint8_t* to) const noexcept {
        std::size_t from = 0;
        for (const auto& segment : segments_) {
            const auto before = segment.at - from;
            detail::copy(to, bytes_.data() + from, before);
            to += before;
            detail::copy(to, segment.data, segment.size);
            to += segment.size;
            from = segment.at;
        }
        detail::copy(to, bytes_.data() + from, bytes_.size() - from);
    }

    detail::GrowingBytes bytes_;
    /// The runs of bytes kept rather than copied, in the order of their
    /// positions
    detail::Segments segments_;
    /// The values that segments point into. Builders nested by reference
    /// share them, so that a builder nested by reference may be destroyed
    /// before its parent is written.
    std::vector<Owner> owned_;
    /// The bytes of the segments, by which the table's positions are past
    /// the builder's bytes
    std::size_t shift_ = 0;
    /// The position of the table being written, which its offsets count
    /// from: 0 for the builder's own
    std::size_t table_ = 0;
    /// The position in the builder's bytes of that table's first offset
    std::size_t slots_ = 0;
    /// The number of fields that the table being written holds, which a
    /// header of `uint32` offsets limits to fewer than 2^30
    std::uint32_t fields_;
    /// The number of its fields set so far
    std::uint32_t set_ = 0;
    /// The fields of a builder that was moved from, whose header is written
    /// again when the first is set, while `fields_` is 0 so that it's
    /// checked for room
    std::uint32_t restart_fields_ = 0;
    std::uint32_t version_;
    Alignment alignment_ = Alignment::one;
    bool versioned_;
    bool sized_;
    bool restarting_ = false;
    OffsetType offset_type_;
};

/// A table to nest with `TableBuilder::set_table()` or in
/// `TableBuilder::set_vector()`: a builder or bytes, which it borrows from a
/// `const` reference or span and owns from an rvalue. A temporary that it
/// can't own, such as a `const Bytes` or a `std::array` returned by value,
/// doesn't convert to one.
///
/// A builder is copied into its parent when it's nested, and its parent
/// takes or shares what it owns. Bytes are copied, or kept until the parent
/// is written when they're long.
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
        return table != nullptr ? table->byte_length() : bytes().size();
    }

    [[nodiscard]] Alignment default_alignment() const noexcept {
        const auto* table = builder();
        return table != nullptr ? table->alignment() : Alignment::four;
    }

    void check_complete() const {
        if (const auto* table = builder()) {
            table->check_complete();
        }
    }

    std::variant<const TableBuilder*, TableBuilder, std::span<const std::uint8_t>, Bytes, std::vector<std::uint8_t>>
        table_;
};

/// The fields of a table that a `TableBuilder` writes in place, as one of its
/// own fields or an item of a vector, which its `set_table(fields, fill)` and
/// `set_vector(count, fields, fill)` give to the function that sets them. Its
/// setters are the builder's, taking the same values and throwing the same
/// errors, and write each field straight into the builder's bytes, so the
/// table needs no builder of its own and is never copied. It's valid only
/// within that function.
class TableBuilder::Item {
public:
    Item(const Item&) = delete;
    Item& operator=(const Item&) = delete;
    ~Item() = default;

    TAFLA_ALWAYS_INLINE void set_int8(std::int8_t value) {
        builder_->set_int8(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint8(std::uint8_t value) {
        builder_->set_uint8(value);
    }

    TAFLA_ALWAYS_INLINE void set_int16(std::int16_t value) {
        builder_->set_int16(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint16(std::uint16_t value) {
        builder_->set_uint16(value);
    }

    TAFLA_ALWAYS_INLINE void set_int32(std::int32_t value) {
        builder_->set_int32(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint32(std::uint32_t value) {
        builder_->set_uint32(value);
    }

    TAFLA_ALWAYS_INLINE void set_int64(std::int64_t value) {
        builder_->set_int64(value);
    }

    TAFLA_ALWAYS_INLINE void set_uint64(std::uint64_t value) {
        builder_->set_uint64(value);
    }

    TAFLA_ALWAYS_INLINE void set_float32(float value) {
        builder_->set_float32(value);
    }

    TAFLA_ALWAYS_INLINE void set_float64(double value) {
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
    TAFLA_ALWAYS_INLINE void set_table(std::size_t fields, Fill&& fill) {
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

// Inlined into the caller, as the scalars' setters are, so that the fields
// that `fill` sets are written by code inlined where they're set
template <std::invocable<TableBuilder::Item&> Fill>
TAFLA_ALWAYS_INLINE void TableBuilder::set_table(std::size_t fields, Fill&& fill) {
    check_room();
    const auto header = detail::header_length(fields, false, false, offset_type_);

    const auto saved = checkpoint();
    try {
        start_in_place(static_cast<std::uint32_t>(fields), header);
        Item item(*this);
        std::invoke(fill, item);
        check_filled();

        // Padded from the parent's length where the table started, as a
        // nested builder is, which moves the table forward by the padding
        const auto start = table_;
        const auto alignment = alignment_;
        restore(saved.counters);
        const std::size_t padding = detail::padding(start - table_, alignment);
        check_length(detail::saturating_sum(table_length(), padding));

        if (padding != 0) [[unlikely]] {
            move_by(padding, saved);
        }

        write_offset(start + padding);
        commit(alignment);
    } catch (...) {
        truncate(saved);
        throw;
    }
}

template <std::invocable<std::size_t, TableBuilder::Item&> Fill>
void TableBuilder::set_vector(std::size_t count, std::size_t fields, Fill&& fill) {
    // The count and the items' ends, which fail before any item is written
    const auto slots = detail::vector_bytes(count, width());
    check_field(slots);
    const auto header = detail::header_length(fields, false, false, offset_type_);

    const auto saved = checkpoint();
    try {
        const auto vector = position();
        const auto ends = bytes_.size() + width();
        // The items' ends are written after each item, so they aren't zeroed
        auto* at = bytes_.extend(static_cast<std::size_t>(slots));
        write_offset(vector);
        detail::store_slot(at, count, offset_type_);

        for (std::size_t index = 0; index < count; ++index) {
            const auto item_start = bytes_.size();
            start_in_place(static_cast<std::uint32_t>(fields), header);
            Item item(*this);
            std::invoke(fill, index, item);
            check_filled();
            restore(saved.counters);

            detail::store_slot(bytes_.data() + ends + index * width(), position() - vector, offset_type_);

            if (index == 0 && count > 1) {
                reserve_items_like(bytes_.size() - item_start, count);
            }
        }

        check_length(table_length());
    } catch (...) {
        truncate(saved);
        throw;
    }
    commit(Alignment::one);
}

inline TableBuilder::TableBuilder(TableBuilder&& other) noexcept
    : bytes_(std::move(other.bytes_)),
      segments_(std::move(other.segments_)),
      owned_(std::move(other.owned_)),
      shift_(other.shift_),
      table_(other.table_),
      slots_(other.slots_),
      fields_(other.fields_),
      set_(other.set_),
      restart_fields_(other.restart_fields_),
      version_(other.version_),
      alignment_(other.alignment_),
      versioned_(other.versioned_),
      sized_(other.sized_),
      restarting_(other.restarting_),
      offset_type_(other.offset_type_) {
    other.restart();
}

inline TableBuilder& TableBuilder::operator=(TableBuilder&& other) noexcept {
    if (this != &other) {
        bytes_ = std::move(other.bytes_);
        segments_ = std::move(other.segments_);
        owned_ = std::move(other.owned_);
        shift_ = other.shift_;
        table_ = other.table_;
        slots_ = other.slots_;
        fields_ = other.fields_;
        set_ = other.set_;
        restart_fields_ = other.restart_fields_;
        version_ = other.version_;
        alignment_ = other.alignment_;
        versioned_ = other.versioned_;
        sized_ = other.sized_;
        restarting_ = other.restarting_;
        offset_type_ = other.offset_type_;
        other.restart();
    }
    return *this;
}

inline void TableBuilder::set_table(NestedTable table) {
    const auto alignment = table.default_alignment();
    set_table(std::move(table), alignment);
}

inline void TableBuilder::set_table(NestedTable table, Alignment alignment) {
    table.check_complete();
    check_room();

    // Padded from the table's length, unlike an array, since it has no count
    const std::size_t padding = detail::padding(table_length(), alignment);
    check_length(detail::saturating_sum(table_length(), detail::saturating_sum(padding, table.byte_length())));

    Room room;
    room.bytes = padding;
    add_room(room, table);
    reserve(room);

    const auto saved = checkpoint();
    try {
        detail::zero(bytes_.extend(padding), padding);
        write_offset(position());
        append_nested(table);
    } catch (...) {
        truncate(saved);
        throw;
    }
    commit(alignment);
}

inline void TableBuilder::set_vector(std::vector<NestedTable> items) {
    std::uint64_t nested_bytes = 0;
    Room room;
    for (const auto& item : items) {
        item.check_complete();
        nested_bytes = detail::saturating_sum(nested_bytes, item.byte_length());
        add_room(room, item);
    }

    const auto slots = detail::vector_bytes(items.size(), width());
    check_field(detail::saturating_sum(slots, nested_bytes));

    room.bytes += static_cast<std::size_t>(slots);
    reserve(room);

    // Keeping an item's owned bytes allocates, so the items written before
    // one that fails are removed
    const auto saved = checkpoint();
    try {
        const auto vector = position();
        const auto ends = bytes_.size() + width();
        write_offset(vector);
        // The items' ends are written after each item, so they aren't zeroed
        auto* at = bytes_.extend(static_cast<std::size_t>(slots));
        detail::store_slot(at, items.size(), offset_type_);

        for (std::size_t index = 0; index < items.size(); ++index) {
            append_nested(items[index]);
            detail::store_slot(bytes_.data() + ends + index * width(), position() - vector, offset_type_);
        }
    } catch (...) {
        truncate(saved);
        throw;
    }
    commit(Alignment::one);
}

inline void TableBuilder::add_room(Room& room, const NestedTable& table) noexcept {
    if (const auto* builder = table.builder()) {
        room.bytes += builder->bytes_.size();
        room.segments += builder->segments_.size();
        room.owned += builder->owned_.size();
    } else if (is_segment(table.bytes().size())) {
        ++room.segments;
        if (!std::holds_alternative<std::span<const std::uint8_t>>(table.table_)) {
            ++room.owned;
        }
    } else {
        room.bytes += table.bytes().size();
    }
}

inline void TableBuilder::append_nested(NestedTable& table) {
    if (const auto* builder = table.builder()) {
        append_builder(*builder);
        if (auto* owned = std::get_if<TableBuilder>(&table.table_)) {
            std::move(owned->owned_.begin(), owned->owned_.end(), std::back_inserter(owned_));
        } else {
            owned_.insert(owned_.end(), builder->owned_.begin(), builder->owned_.end());
        }
        return;
    }

    const auto bytes = table.bytes();
    if (!is_segment(bytes.size())) {
        detail::copy(bytes_.extend(bytes.size()), bytes.data(), bytes.size());
        return;
    }

    if (auto* owned = std::get_if<Bytes>(&table.table_)) {
        owned_.push_back(std::make_shared<detail::OwnedValue<Bytes>>(std::move(*owned)));
    } else if (auto* owned_vector = std::get_if<std::vector<std::uint8_t>>(&table.table_)) {
        owned_.push_back(std::make_shared<detail::OwnedValue<std::vector<std::uint8_t>>>(std::move(*owned_vector)));
    }
    add_segment(bytes.data(), bytes.size());
}

}  // namespace tafla

#endif
