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
#include <initializer_list>
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

/// A container that a `NestedTable` would view after the call that it's
/// given to ends, such as a `std::array` or a `const std::vector` returned by
/// value, which it can't own. `NestedTable` deletes its constructor from
/// these, so that a call with one doesn't compile, where it would otherwise
/// convert to a `std::span` of it. Views, such as spans, and containers that
/// it owns from an rvalue are given as they are.
template <class Range>
concept Temporary = std::ranges::contiguous_range<Range> && !std::ranges::borrowed_range<Range>;

/// The bytes that a `TableBuilder` writes its tables into, as their fields
/// are set: a `std::vector` of its own or the caller's, which grows, or bytes
/// of the caller's, which don't.
///
/// A vector is kept as long as its capacity, so that every byte up to it can
/// be written where the builder writes it, whether or not the bytes before it
/// are: a field's offset is written once its bytes are, and a vector's item
/// ends once its items are. It's cut to the table's length when it's handed
/// back.
class TableBuffer {
public:
    /// The least room that a builder's first bytes are allocated with,
    /// which holds a small table whole, as Rust's builder starts
    static constexpr std::size_t kFirstCapacity = 256;
    /// The room that a builder's first bytes are allocated with for each
    /// field of its table, beyond its header, up to `kMostRoomForFields`, so
    /// that a table of many fields grows its bytes fewer times
    static constexpr std::size_t kRoomForAField = 64;
    static constexpr std::size_t kMostRoomForFields = 64 << 10;
    /// The capacity below which the bytes grow fourfold rather than twofold,
    /// which grows a table of many short fields fewer times, and above which
    /// they waste no more than half their capacity
    static constexpr std::size_t kFourfoldBelow = 64 << 10;

    /// No bytes, which the first table allocates
    TableBuffer() noexcept = default;

    /// The caller's vector, whose bytes the builder writes over, as far as
    /// its capacity
    explicit TableBuffer(std::vector<std::uint8_t>&& vector) : vector_(std::move(vector)) {
        // Within its capacity, so it allocates nothing
        vector_.resize(vector_.capacity());
        data_ = vector_.data();
        capacity_ = vector_.size();
    }

    /// The caller's bytes, which are never grown
    explicit TableBuffer(std::span<std::uint8_t> bytes) noexcept
        : data_(bytes.data()), capacity_(bytes.size()), fixed_(true) {}

    /// Moves `other`'s bytes, and leaves it without any
    TableBuffer(TableBuffer&& other) noexcept
        : vector_(std::move(other.vector_)),
          data_(std::exchange(other.data_, nullptr)),
          size_(std::exchange(other.size_, 0)),
          capacity_(std::exchange(other.capacity_, 0)),
          fixed_(std::exchange(other.fixed_, false)) {}

    TableBuffer& operator=(TableBuffer&& other) noexcept {
        if (this != &other) {
            vector_ = std::move(other.vector_);
            data_ = std::exchange(other.data_, nullptr);
            size_ = std::exchange(other.size_, 0);
            capacity_ = std::exchange(other.capacity_, 0);
            fixed_ = std::exchange(other.fixed_, false);
        }
        return *this;
    }

    TableBuffer(const TableBuffer&) = delete;
    TableBuffer& operator=(const TableBuffer&) = delete;
    ~TableBuffer() = default;

    [[nodiscard]] std::uint8_t* data() noexcept {
        return data_;
    }

    [[nodiscard]] const std::uint8_t* data() const noexcept {
        return data_;
    }

    [[nodiscard]] std::size_t size() const noexcept {
        return size_;
    }

    /// The room for the next table, of `fields` fields after a header of
    /// `header` bytes: the header, and room for its fields, unless the bytes
    /// have more
    [[nodiscard]] static std::size_t first_capacity(std::size_t header, std::size_t fields) noexcept {
        const auto room = static_cast<std::size_t>(
            (std::min<std::uint64_t>)(saturating_product(fields, kRoomForAField), kMostRoomForFields));
        // A header past what a 32-bit host addresses saturates, and fails
        // to be allocated
        const auto capacity = header > SIZE_MAX - room ? SIZE_MAX : header + room;
        return (std::max)(kFirstCapacity, capacity);
    }

    /// Makes room for a new table of `header` bytes of header and `capacity`
    /// in all, discarding the bytes written, which it leaves as they were if
    /// that fails. The caller's bytes need only hold the header.
    void start(std::size_t header, std::size_t capacity) {
        if (fixed_) {
            if (header > capacity_) {
                throw_too_short(header, capacity_);
            }
        } else if (capacity > capacity_) {
            replace(capacity, 0);
        }
        size_ = 0;
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
    TAFLA_ALWAYS_INLINE void reserve_more(std::size_t count) {
        if (count > capacity_ - size_) [[unlikely]] {
            grow(count);
        }
    }

    /// Makes room for `count` more bytes, as `reserve_more()` does, unless
    /// the bytes are the caller's, which can't grow: room that the bytes
    /// may need, rather than room that they do
    void reserve_likely(std::size_t count) {
        if (!fixed_) {
            reserve_more(count);
        }
    }

    /// Removes the bytes past the first `size`
    void truncate(std::size_t size) noexcept {
        size_ = size;
    }

    /// Returns the first `size()` bytes as a vector, the builder's own or the
    /// caller's, without copying them, or a copy of the caller's bytes, and
    /// leaves the buffer without any
    [[nodiscard]] std::vector<std::uint8_t> release() {
        std::vector<std::uint8_t> released;
        if (fixed_) {
            released.assign(data_, data_ + size_);
        } else {
            // Shorter, so it allocates nothing
            vector_.resize(size_);
            released = std::move(vector_);
        }
        *this = TableBuffer();
        return released;
    }

private:
    /// Grows the bytes to hold `count` more: fourfold, or twofold from
    /// `kFourfoldBelow`, or to what they need if that's more
    TAFLA_COLD void grow(std::size_t count) {
        const auto needed = count > SIZE_MAX - size_ ? SIZE_MAX : size_ + count;
        if (fixed_) {
            throw_too_short(needed, capacity_);
        }

        const std::size_t factor = capacity_ < kFourfoldBelow ? 4 : 2;
        const auto grown = capacity_ > SIZE_MAX / factor ? SIZE_MAX : factor * capacity_;
        const auto most = vector_.max_size();
        if (needed > most) {
            throw std::bad_alloc();
        }
        replace((std::max)({needed, (std::min)(grown, most), kFirstCapacity}), size_);
    }

    /// Replaces the bytes with `capacity` new ones, holding the first `keep`
    /// of the old, and leaves them as they were if that fails
    void replace(std::size_t capacity, std::size_t keep) {
        std::vector<std::uint8_t> grown;
        if (capacity > grown.max_size()) {
            throw std::bad_alloc();
        }
        grown.reserve(capacity);
        grown.assign(data_, data_ + keep);
        grown.resize(capacity);

        vector_ = std::move(grown);
        data_ = vector_.data();
        capacity_ = capacity;
    }

    /// The bytes of the builder's own or the caller's vector, unless they're
    /// the caller's bytes
    std::vector<std::uint8_t> vector_;
    std::uint8_t* data_ = nullptr;
    /// The bytes written
    std::size_t size_ = 0;
    /// The bytes that can be written: the vector's size, or the caller's
    std::size_t capacity_ = 0;
    /// Whether the bytes are the caller's, which aren't grown
    bool fixed_ = false;
};

}  // namespace detail

/// Builds a table, which a `TableReader` reads back.
///
/// A table is a header with one offset per field, after the table's total
/// length and version when its layout has them, followed by the fields in
/// the order they're set. Arrays, strings and vectors start with their
/// count, and a vector then holds the end of each item. Arrays of scalars
/// and of structs are padded so their elements are aligned from the start of
/// the table. Fields are read back by index with `TableReader::root_offset()`,
/// or `TableReader::field_offset()` for a table nested in another.
///
/// The builder writes the table forward into one buffer as its fields are
/// set: the header when it's started, and then each field's bytes after the
/// last's, and its offset into the header. Each setter copies its value, so
/// a value may change or be destroyed once its setter returns. The table
/// comes out three ways:
///
/// - `finish()` lends it, as a view of the buffer;
/// - `std::move(builder).into_buffer()` hands over the buffer, a
///   `std::vector` as long as the table, with its spare room;
/// - `to_bytes()` copies it into `Bytes` of its exact length.
///
/// The buffer is a vector of the builder's own, which starts with room for
/// 64 bytes a field, at least 256 and up to 64 KiB, and grows fourfold, or
/// twofold past 64 KiB. `with_buffer()` takes the caller's vector instead,
/// whose capacity it keeps, and `with_bytes()` the caller's bytes, which it
/// never grows. `reset()` starts the next table in the same buffer, so that a
/// loop allocates only while its tables outgrow it.
///
/// A setter throws a `BuildError`, and leaves the builder as it was, when the
/// table already holds all its fields or the field would make it too long
/// for the layout's offset type: longer than 255 bytes, 64 KiB or 4 GiB. One
/// whose allocation fails throws `std::bad_alloc`, and one that outgrows the
/// caller's bytes `std::invalid_argument`, and leaves the builder as it was
/// too.
class TableBuilder {
public:
    /// Starts a table of `fields` fields in `layout`, which its readers must
    /// use too, writing its header: an offset for each field, and the slots
    /// of the layout's length and version, with the layout's version when it
    /// has one. Nested tables use the default layout.
    ///
    /// Throws a `BuildError` if the layout's version doesn't fit in its
    /// offset type, or the header alone is too long for it, and
    /// `std::bad_alloc` if the buffer can't be allocated.
    template <bool Versioned>
    TableBuilder(std::size_t fields, TableLayout<Versioned> layout)
        : TableBuilder(detail::TableBuffer(), fields, layout) {}

    /// Starts a table as the constructor does, in `buffer`, the caller's
    /// vector, whose bytes it writes over and whose capacity it keeps, adding
    /// to it if it has less room than the builder starts with. A vector that
    /// the caller reserved room in, or that held another table, holds a table
    /// that fits without allocating again. `std::move(builder).into_buffer()`
    /// gives it back.
    ///
    /// Throws as the constructor does, and drops `buffer` then.
    template <bool Versioned>
    [[nodiscard]] static TableBuilder with_buffer(std::vector<std::uint8_t> buffer, std::size_t fields,
                                                  TableLayout<Versioned> layout) {
        return TableBuilder(detail::TableBuffer(std::move(buffer)), fields, layout);
    }

    /// Starts a table as the constructor does, in `bytes`, the caller's,
    /// which must outlive the builder and the table that `finish()` lends,
    /// and which the builder never grows, so that it allocates nothing. A
    /// setter of a field that the bytes can't hold throws
    /// `std::invalid_argument`, as `TableWriter::finish_into()` does for bytes
    /// shorter than its table, and leaves the builder as it was, so that it
    /// can still `reset()`.
    ///
    /// Throws as the constructor does, or `std::invalid_argument` if `bytes`
    /// can't hold the header.
    template <bool Versioned>
    [[nodiscard]] static TableBuilder with_bytes(std::span<std::uint8_t> bytes, std::size_t fields,
                                                 TableLayout<Versioned> layout) {
        return TableBuilder(detail::TableBuffer(bytes), fields, layout);
    }

    /// Moves `other`'s buffer, and leaves it as a builder that was just
    /// started, with its layout and number of fields, which throws a
    /// `BuildError` until every field is set again. It writes its header
    /// again, into a buffer of its own, when its first field is set, or its
    /// table is written, so that moving it allocates nothing.
    TableBuilder(TableBuilder&& other) noexcept;
    TableBuilder& operator=(TableBuilder&& other) noexcept;
    TableBuilder(const TableBuilder&) = delete;
    TableBuilder& operator=(const TableBuilder&) = delete;
    ~TableBuilder() = default;

    /// Starts the next table, of `fields` fields in `layout`, in the same
    /// buffer, discarding the table written so far, finished or not. It
    /// allocates only if the buffer has less room than a new builder starts
    /// with.
    ///
    /// Throws as the constructor does, leaving the builder as it was.
    template <bool Versioned>
    void reset(std::size_t fields, TableLayout<Versioned> layout) {
        const auto header = detail::checked_header(fields, layout);
        bytes_.start(header, detail::TableBuffer::first_capacity(header, fields));
        start(fields, header, detail::header_version(layout), layout.is_sized(), layout.offset_type());
    }

    /// Writes a sized table's length into its header, and lends the table:
    /// the builder's buffer, as long as the table. The view is valid until
    /// the builder is next changed, reset, moved or destroyed.
    ///
    /// Throws a `BuildError` if fewer fields were set than the table holds,
    /// and, for a builder that was moved from, as its first setter would.
    [[nodiscard]] std::span<const std::uint8_t> finish() {
        check_complete();
        if (restarting_) {
            restart_now();
        }

        if (sized_) {
            detail::store_slot(bytes_.data(), bytes_.size(), offset_type_);
        }
        return {bytes_.data(), bytes_.size()};
    }

    /// Writes the table as `finish()` does, and hands over the buffer: a
    /// `std::vector` as long as the table, with the buffer's capacity, which
    /// is the vector that `with_buffer()` took, if it did. The bytes of
    /// `with_bytes()` are copied into a new vector. The builder is left as a
    /// moved-from builder is.
    ///
    /// Throws as `finish()` does, leaving the builder as it was.
    [[nodiscard]] std::vector<std::uint8_t> into_buffer() && {
        static_cast<void>(finish());
        auto buffer = bytes_.release();
        restart();
        return buffer;
    }

    /// Copies the table into `Bytes` of its exact length, as `finish()` would
    /// write it, leaving the builder as it is.
    ///
    /// Throws a `BuildError` if fewer fields were set than the table holds.
    [[nodiscard]] Bytes to_bytes() const {
        check_complete();

        auto bytes = Bytes::for_overwrite(byte_length());
        copy_table(bytes.data());
        return bytes;
    }

    /// The table's byte length with the fields set so far
    [[nodiscard]] std::size_t byte_length() const noexcept {
        return restarting_ ? header_length(restart_fields_) : bytes_.size();
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

    /// Sets an array field of `std::int8_t`, copying the values of a span, or
    /// of anything that converts to one, such as a `std::vector` or a
    /// `std::array`. So do the other arrays' setters.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_int8_array(std::span<const std::int8_t> values) {
        set_array(values);
    }

    void set_uint8_array(std::span<const std::uint8_t> values) {
        set_array(values);
    }

    void set_int16_array(std::span<const std::int16_t> values) {
        set_array(values);
    }

    void set_uint16_array(std::span<const std::uint16_t> values) {
        set_array(values);
    }

    void set_int32_array(std::span<const std::int32_t> values) {
        set_array(values);
    }

    void set_uint32_array(std::span<const std::uint32_t> values) {
        set_array(values);
    }

    void set_int64_array(std::span<const std::int64_t> values) {
        set_array(values);
    }

    void set_uint64_array(std::span<const std::uint64_t> values) {
        set_array(values);
    }

    void set_float32_array(std::span<const float> values) {
        set_array(values);
    }

    void set_float64_array(std::span<const double> values) {
        set_array(values);
    }

    /// Sets an array field of bools, written as a `uint8` array of 1 for each
    /// true value and 0 for each false one, from a span or `Bools`, as those
    /// that `TableReader::get_bool_array()` reads.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_bool_array(std::span<const bool> values) {
        set_array(values);
    }

    /// Sets a string field, from its UTF-8 bytes.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_string(std::string_view string) {
        check_field(detail::saturating_sum(width(), string.size()));
        put_counted(0, string.size(), string.data(), string.size(), Alignment::one);
    }

    void set_string(const char* string) {
        set_string(std::string_view(string));
    }

    /// Sets a struct field from its bytes: its scalars, little-endian, one
    /// after another, which are written as they are, without padding.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_struct(std::span<const std::uint8_t> bytes) {
        check_field(bytes.size());

        const auto start = bytes_.size();
        auto* at = bytes_.extend(bytes.size());
        write_offset(start);
        detail::copy(at, bytes.data(), bytes.size());
        commit(Alignment::one);
    }

    /// Sets an array of structs of `size` bytes each, from their bytes one
    /// after another, which it counts. It's padded so that the structs start
    /// at a multiple of `alignment` from the start of the table, which is
    /// the size of the structs' largest scalar.
    ///
    /// Throws a `BuildError` if `size` is 0 or the bytes aren't a whole
    /// number of structs, or as the scalars' setters do.
    void set_struct_array(std::span<const std::uint8_t> bytes, std::size_t size, Alignment alignment) {
        const auto padding = check_struct_array(bytes.size(), size, alignment);
        put_counted(padding, bytes.size() / size, bytes.data(), bytes.size(), alignment);
    }

    /// Sets a vector of strings, from their UTF-8 bytes, after the end of
    /// each, which lets readers find any string without reading the others.
    /// It takes no more bytes for each string than its end, where a vector of
    /// tables of one string takes each table's offset and the string's count
    /// too. The strings are views, `std::string`s, such as those of a
    /// `std::vector`, or a list of either: `{"sensor", "outdoor"}`.
    ///
    /// Throws a `BuildError` as the scalars' setters do.
    void set_string_vector(std::span<const std::string_view> strings) {
        put_string_vector(strings);
    }

    void set_string_vector(std::span<const std::string> strings) {
        put_string_vector(strings);
    }

    void set_string_vector(std::initializer_list<std::string_view> strings) {
        put_string_vector(strings);
    }

    /// Embeds a table, from its builder or its bytes, without a length
    /// prefix. Readers find its fields with `TableReader::field_offset()` from
    /// the field's position, which reads offsets of the parent's offset type,
    /// so the table should have no header and the parent's offset type:
    /// that's the default layout in a parent of `uint32` offsets.
    ///
    /// A builder must have all its fields set, and is copied into this table
    /// at once, in its own layout, so it may then change or be destroyed.
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
    void set_table(const NestedTable& table);

    /// Embeds a table as the overload without an alignment does, padded to
    /// start at a multiple of `alignment`, such as `Alignment::eight` for a
    /// table of 64-bit arrays given as bytes
    void set_table(const NestedTable& table, Alignment alignment);

    /// Writes a vector of tables, from their builders or their bytes, after
    /// the end of each, which lets readers find any item without reading the
    /// others. Builders must have all their fields set, and are copied into
    /// this table as `set_table()` copies them. Items aren't padded, so their
    /// arrays are read in place only when they happen to be aligned.
    ///
    /// Throws a `BuildError` if a builder has fields left to set, or as the
    /// scalars' setters do, and leaves the builder as it was, as it does if
    /// an allocation fails.
    void set_vector(std::span<const NestedTable> items);

    class Item;

    /// Writes a table of `fields` fields nested in this one, in place, as a
    /// nested builder would be, but without a builder of its own: `fill` sets
    /// its fields, in order, on the `Item` that it's given, and each is
    /// written straight into this builder's buffer. The table is padded to
    /// start at a multiple of its largest alignment, as `set_table()` pads a
    /// builder: it's written where this table's next field goes, and moved
    /// forward by its padding once its fields are set, which changes none of
    /// its offsets, since they count from its own start. The overload with an
    /// `Alignment` pads it first, which avoids the move.
    ///
    /// The builder itself mustn't be used within `fill`, only the `Item`.
    ///
    /// Throws a `BuildError` if `fill` leaves fields of the table unset, or as
    /// the setters do, and passes on what `fill` throws, leaving the builder
    /// as it was either way.
    template <std::invocable<Item&> Fill>
    void set_table(std::size_t fields, Fill&& fill);

    /// Writes a table in place as the overload without an alignment does,
    /// padded first to start at a multiple of `alignment`, before `fill` sets
    /// its fields: the largest alignment of its arrays and nested tables,
    /// which its fields fix, since each must be set. It's then never moved,
    /// unless its fields need a larger alignment, as they do from a smaller
    /// `alignment` than theirs. From a larger one, it's padded more than the
    /// overload without an alignment pads it, as `set_table(table, alignment)`
    /// pads a builder.
    ///
    /// Throws as the overload without an alignment does.
    template <std::invocable<Item&> Fill>
    void set_table(std::size_t fields, Alignment alignment, Fill&& fill);

    /// Writes a vector of `count` tables of `fields` fields each, in place,
    /// as `set_table(fields, fill)` writes one: `fill(index, item)` sets the
    /// fields of item `index`, for each in order. Items aren't padded, as
    /// `set_vector()` doesn't pad them.
    ///
    /// Throws a `BuildError` if the vector's count makes the table too long
    /// before any item is written, with the length that its count and its
    /// items' ends make it, if `fill` leaves an item's fields unset, or as
    /// the setters do, and passes on what `fill` throws, leaving the builder
    /// as it was either way.
    template <std::invocable<std::size_t, Item&> Fill>
    void set_vector(std::size_t count, std::size_t fields, Fill&& fill);

private:
    friend class NestedTable;

    /// The most room that a vector written in place makes for its items
    /// after its first, as `reserve_items_like()` describes. Without that
    /// room, the comparison's vector of 100 nested tables, of 3.2 KB, took 8%
    /// longer, growing its bytes twice more, and with room for twice the
    /// least that the other items write, their headers, as the recording
    /// builder bounded its records, 5% longer.
    static constexpr std::uint64_t kItemsRoom = 64 << 10;

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
    };

    template <bool Versioned>
    TableBuilder(detail::TableBuffer&& buffer, std::size_t fields, TableLayout<Versioned> layout)
        : bytes_(std::move(buffer)) {
        reset(fields, layout);
    }

    /// Starts a table in the buffer, which has room for its header of
    /// `header` bytes
    void start(std::size_t fields, std::uint32_t header, std::optional<std::uint32_t> version, bool sized,
               OffsetType offset_type) noexcept {
        // The header holds an offset of at least a byte per field
        fields_ = static_cast<std::uint32_t>(fields);
        set_ = 0;
        restart_fields_ = 0;
        version_ = version.value_or(0);
        alignment_ = Alignment::one;
        versioned_ = version.has_value();
        sized_ = sized;
        restarting_ = false;
        offset_type_ = offset_type;
        table_ = 0;
        write_header(header);
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

    /// Writes the header of `header` bytes, for which the buffer has room:
    /// its length's slot, which `finish()` writes, its version and its
    /// offsets, written as each field is set. It isn't zeroed first, as
    /// `start_in_place()` describes.
    void write_header(std::uint32_t header) noexcept {
        auto* at = bytes_.extend(header);
        if (versioned_) {
            detail::store_slot(at + (sized_ ? width() : 0), version_, offset_type_);
        }
        slots_ = ((sized_ ? 1U : 0U) + (versioned_ ? 1U : 0U)) * width();
    }

    /// Leaves the builder as its constructor did, without fields, after its
    /// buffer was moved. Its header is written again, into a buffer of its
    /// own, when its first field is set or its table is written, so that
    /// moving the builder doesn't allocate.
    void restart() noexcept {
        restart_fields_ = restarting_ ? restart_fields_ : fields_;
        restarting_ = true;
        fields_ = 0;
        set_ = 0;
        table_ = 0;
        alignment_ = Alignment::one;
    }

    /// Writes the header of a builder that was moved from, into a buffer of
    /// its own
    TAFLA_COLD void restart_now() {
        const auto fields = restart_fields_;
        const auto header = header_length(fields);
        bytes_.start(header, detail::TableBuffer::first_capacity(header, fields));
        start(fields, header, versioned_ ? std::optional<std::uint32_t>(version_) : std::nullopt, sized_, offset_type_);
    }

    /// The length of the table being written, which is where its next field
    /// goes
    [[nodiscard]] std::size_t table_length() const noexcept {
        return bytes_.size() - table_;
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
        return {counters(), bytes_.size()};
    }

    void truncate(const Checkpoint& checkpoint) noexcept {
        restore(checkpoint.counters);
        bytes_.truncate(checkpoint.bytes);
    }

    /// Checks that the builder's table has every field set, for it to be
    /// written or nested
    void check_complete() const {
        if (restarting_) {
            if (restart_fields_ != 0) {
                throw BuildError::incomplete(0, restart_fields_);
            }
            return;
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
    /// set again, from the first, unless it has none
    TAFLA_COLD void restart_or_throw() {
        if (restarting_) {
            restart_now();
        }
        if (set_ == fields_) {
            throw BuildError::too_many_fields(fields_);
        }
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
    /// in the buffer
    TAFLA_ALWAYS_INLINE void write_offset(std::size_t position) noexcept {
        detail::store_slot(bytes_.data() + table_ + slots_ + std::size_t{set_} * width(), position - table_,
                           offset_type_);
    }

    /// Counts the field just written, which needs `alignment` from the
    /// table's start
    TAFLA_ALWAYS_INLINE void commit(Alignment alignment) noexcept {
        ++set_;
        alignment_ = (std::max)(alignment_, alignment);
    }

    template <class T>
    TAFLA_ALWAYS_INLINE void set_scalar(T value) {
        check_field(sizeof(T));

        const auto start = bytes_.size();
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
            put_counted(padding, values.size(), values.data(), values.size_bytes(), alignment);
        } else {
            put_swapped(padding, values);
        }
    }

    /// Writes a field of a count, `count`, and then `size` bytes from `data`,
    /// after `padding` zeros, and its offset, once the setter checked it.
    ///
    /// Forced inline, so that each setter that calls it has a copy fitted
    /// to its kind of value. Called out of line, as Clang chose in a loop of
    /// `set_string()` calls, it made setting 20,000 strings of 100 bytes
    /// take 1.56 times as long in #212's builder, 2,000 of 255 bytes 1.43
    /// times, and the comparison's document of 100 rows 1.10 times.
    TAFLA_ALWAYS_INLINE void put_counted(std::size_t padding, std::size_t count, const void* data, std::size_t size,
                                         Alignment alignment) {
        const auto start = bytes_.size() + padding;
        auto* at = bytes_.extend(padding + width() + size);

        detail::zero(at, padding);
        write_offset(start);
        detail::store_slot(at + padding, count, offset_type_);
        detail::copy(at + padding + width(), data, size);
        commit(alignment);
    }

    /// Writes an array whose elements are swapped to little-endian one by
    /// one, on a big-endian host
    template <class T>
    void put_swapped(std::size_t padding, std::span<const T> values) {
        const auto start = bytes_.size() + padding;
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

    /// Writes a field of a vector of `strings`: its count, the end of each
    /// string, counted from the vector's position, and the strings' bytes.
    /// It makes room for the whole field first, so that nothing after that
    /// throws.
    template <class Strings>
    void put_string_vector(const Strings& strings) {
        const auto length = detail::string_vector_bytes(strings, width());
        check_field(length);
        // Within the table's length, which `check_field()` limited
        bytes_.reserve_more(static_cast<std::size_t>(length));

        const auto vector = bytes_.size();
        const auto count = strings.size();
        auto* at = bytes_.extend((count + 1) * width());
        write_offset(vector);
        detail::store_slot(at, count, offset_type_);

        auto end = vector + width();
        for (const auto& string : strings) {
            const std::string_view view(string);
            detail::copy(bytes_.extend(view.size()), view.data(), view.size());
            detail::store_slot(bytes_.data() + end, bytes_.size() - vector, offset_type_);
            end += width();
        }
        commit(Alignment::one);
    }

    /// Copies the complete table, as `finish()` would write it, to `to`,
    /// which has room for `byte_length()` bytes
    void copy_table(std::uint8_t* to) const noexcept {
        const auto length = byte_length();
        if (restarting_) {
            // A table of no fields, whose builder was moved from: its slots,
            // two of 4 bytes at most
            std::array<std::uint8_t, 8> slots{};
            if (sized_) {
                detail::store_slot(slots.data(), length, offset_type_);
            }
            if (versioned_) {
                detail::store_slot(slots.data() + (sized_ ? width() : 0), version_, offset_type_);
            }
            detail::copy(to, slots.data(), length);
            return;
        }

        detail::copy(to, bytes_.data(), length);
        // A sized table holds its length's slot, so it's never empty
        if (sized_ && length != 0) {
            detail::store_slot(to, length, offset_type_);
        }
    }

    /// Makes room, after the first of a vector's `count` items wrote `first`
    /// bytes, for the others: as many bytes each as the first's, so that
    /// items alike, as a vector's usually are, grow the buffer at most once,
    /// but for no more than `kItemsRoom`, so that a first item far longer
    /// than the others leaves little room unused, and the buffer grows past it
    /// as it would anyway
    void reserve_items_like(std::size_t first, std::size_t count) {
        const auto like_first = detail::saturating_product(count - 1, first);
        bytes_.reserve_likely(static_cast<std::size_t>((std::min<std::uint64_t>)(like_first, kItemsRoom)));
    }

    /// Writes item `index` of a vector in place that already makes its table
    /// too long, as `set_vector(count, fields, fill)` writes one, and removes
    /// it, returning its length, which the vector's error counts. It's called
    /// rather than inlined, since no table that fits calls it.
    template <class Fill>
    TAFLA_COLD std::uint64_t discard_item(std::size_t index, std::size_t fields, std::uint32_t header, Fill& fill);

    /// Starts a table of `fields` fields in place, whose header of `header`
    /// bytes holds only their offsets, at the next byte, and counts its
    /// fields rather than its parent's.
    ///
    /// Its header isn't zeroed: each offset is written when its field is
    /// set, and a table is written or nested only once every field is.
    /// Zeroing the headers made building the comparison's record 28% slower
    /// in #212's builder, a vector of 100 nested builders 23%, and builders
    /// nested 3 deep 12%.
    TAFLA_ALWAYS_INLINE void start_in_place(std::uint32_t fields, std::uint32_t header) {
        const auto start = bytes_.size();
        bytes_.extend(header);
        restore({start, 0, fields, 0, Alignment::one});
    }

    /// Moves the table written in place from `start` to the buffer's end
    /// forward by `padding`, and zeros the padding before it. None of the
    /// comparison's tables needs it, so it's called rather than inlined.
    TAFLA_COLD void move_by(std::size_t padding, std::size_t start) {
        const auto length = bytes_.size() - start;
        bytes_.extend(padding);
        auto* table = bytes_.data() + start;
        std::memmove(table + padding, table, length);
        detail::zero(table, padding);
    }

    detail::TableBuffer bytes_;
    /// The position in the buffer of the table being written, which its
    /// offsets count from: 0 for the builder's own
    std::size_t table_ = 0;
    /// The position in that table of its first offset
    std::size_t slots_ = 0;
    /// The number of fields that the table being written holds, which a
    /// header of `uint32` offsets limits to fewer than 2^30
    std::uint32_t fields_ = 0;
    /// The number of its fields set so far
    std::uint32_t set_ = 0;
    /// The fields of a builder that was moved from, whose header is written
    /// again when the first is set, while `fields_` is 0 so that it's
    /// checked for room
    std::uint32_t restart_fields_ = 0;
    std::uint32_t version_ = 0;
    Alignment alignment_ = Alignment::one;
    bool versioned_ = false;
    bool sized_ = false;
    bool restarting_ = false;
    OffsetType offset_type_ = OffsetType::uint32;
};

/// A table to nest with `TableBuilder::set_table()` or in
/// `TableBuilder::set_vector()`: a builder or bytes, which it borrows from a
/// `const` reference or span and owns from an rvalue, so that a
/// `std::vector` of them can hold items built in a loop. A temporary that it
/// can't own, such as a `const Bytes` or a `std::array` returned by value,
/// doesn't convert to one.
///
/// A builder or bytes are copied into the parent when they're nested.
class NestedTable {
public:
    // NOLINTBEGIN(google-explicit-constructor): each converts as an argument
    NestedTable(TableBuilder&& builder) noexcept : table_(std::move(builder)) {}

    NestedTable(const TableBuilder& builder) noexcept : table_(&builder) {}

    NestedTable(std::span<const std::uint8_t> bytes) noexcept : table_(bytes) {}

    NestedTable(const std::vector<std::uint8_t>& bytes) noexcept : table_(std::span<const std::uint8_t>(bytes)) {}

    NestedTable(const Bytes& bytes) noexcept : table_(bytes.span()) {}

    NestedTable(std::vector<std::uint8_t>&& bytes) noexcept : table_(std::move(bytes)) {}

    NestedTable(Bytes&& bytes) noexcept : table_(std::move(bytes)) {}

    NestedTable(const TableBuilder&& builder) = delete;

    // Out of line, so that GCC 15 for 32-bit x86 doesn't warn, with
    // -Wmaybe-uninitialized, of the other alternatives of a table that a
    // `std::vector` of tables moves as it grows. Only growing moves a table.
    TAFLA_NOINLINE NestedTable(NestedTable&& other) noexcept : table_(std::move(other.table_)) {}

    TAFLA_NOINLINE NestedTable& operator=(NestedTable&& other) noexcept {
        table_ = std::move(other.table_);
        return *this;
    }

    NestedTable(const NestedTable&) = delete;
    NestedTable& operator=(const NestedTable&) = delete;
    ~NestedTable() = default;

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

    /// Copies the table to `to`, which has room for `byte_length()` bytes
    void copy_to(std::uint8_t* to) const noexcept {
        if (const auto* table = builder()) {
            table->copy_table(to);
        } else {
            const auto table_bytes = bytes();
            detail::copy(to, table_bytes.data(), table_bytes.size());
        }
    }

    std::variant<const TableBuilder*, TableBuilder, std::span<const std::uint8_t>, Bytes, std::vector<std::uint8_t>>
        table_;
};

/// The fields of a table that a `TableBuilder` writes in place, as one of its
/// own fields or an item of a vector, which its `set_table(fields, fill)` and
/// `set_vector(count, fields, fill)` give to the function that sets them. Its
/// setters are the builder's, taking the same values and throwing the same
/// errors, and write each field straight into the builder's buffer, so the
/// table needs no builder of its own and is never copied, unless it's moved
/// by its padding. It's valid only within that function.
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

    void set_uint8_array(std::span<const std::uint8_t> values) {
        builder_->set_uint8_array(values);
    }

    void set_int16_array(std::span<const std::int16_t> values) {
        builder_->set_int16_array(values);
    }

    void set_uint16_array(std::span<const std::uint16_t> values) {
        builder_->set_uint16_array(values);
    }

    void set_int32_array(std::span<const std::int32_t> values) {
        builder_->set_int32_array(values);
    }

    void set_uint32_array(std::span<const std::uint32_t> values) {
        builder_->set_uint32_array(values);
    }

    void set_int64_array(std::span<const std::int64_t> values) {
        builder_->set_int64_array(values);
    }

    void set_uint64_array(std::span<const std::uint64_t> values) {
        builder_->set_uint64_array(values);
    }

    void set_float32_array(std::span<const float> values) {
        builder_->set_float32_array(values);
    }

    void set_float64_array(std::span<const double> values) {
        builder_->set_float64_array(values);
    }

    void set_bool_array(std::span<const bool> values) {
        builder_->set_bool_array(values);
    }

    void set_string(std::string_view string) {
        builder_->set_string(string);
    }

    void set_string(const char* string) {
        builder_->set_string(string);
    }

    void set_struct(std::span<const std::uint8_t> bytes) {
        builder_->set_struct(bytes);
    }

    void set_struct_array(std::span<const std::uint8_t> bytes, std::size_t size, Alignment alignment) {
        builder_->set_struct_array(bytes, size, alignment);
    }

    void set_string_vector(std::span<const std::string_view> strings) {
        builder_->set_string_vector(strings);
    }

    void set_string_vector(std::span<const std::string> strings) {
        builder_->set_string_vector(strings);
    }

    void set_string_vector(std::initializer_list<std::string_view> strings) {
        builder_->set_string_vector(strings);
    }

    void set_table(const NestedTable& table) {
        builder_->set_table(table);
    }

    void set_table(const NestedTable& table, Alignment alignment) {
        builder_->set_table(table, alignment);
    }

    template <std::invocable<Item&> Fill>
    TAFLA_ALWAYS_INLINE void set_table(std::size_t fields, Fill&& fill) {
        builder_->set_table(fields, std::forward<Fill>(fill));
    }

    template <std::invocable<Item&> Fill>
    TAFLA_ALWAYS_INLINE void set_table(std::size_t fields, Alignment alignment, Fill&& fill) {
        builder_->set_table(fields, alignment, std::forward<Fill>(fill));
    }

    void set_vector(std::span<const NestedTable> items) {
        builder_->set_vector(items);
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
    set_table(fields, Alignment::one, std::forward<Fill>(fill));
}

template <std::invocable<TableBuilder::Item&> Fill>
TAFLA_ALWAYS_INLINE void TableBuilder::set_table(std::size_t fields, Alignment alignment, Fill&& fill) {
    check_room();
    const auto header = detail::header_length(fields, false, false, offset_type_);

    const auto saved = checkpoint();
    try {
        // Padded for the alignment given, from the parent's length where the
        // table starts, as a nested builder is
        const auto declared = detail::padding(table_length(), alignment);
        detail::zero(bytes_.extend(declared), declared);

        start_in_place(static_cast<std::uint32_t>(fields), header);
        Item item(*this);
        std::invoke(fill, item);
        check_filled();

        // Moved forward by the padding that its fields need beyond that,
        // which a multiple of a larger alignment than theirs needs none of
        const auto start = table_;
        const auto needed = alignment_;
        restore(saved.counters);
        const auto padding = detail::padding(start - table_, needed);
        check_length(detail::saturating_sum(table_length(), padding));

        if (padding != 0) [[unlikely]] {
            move_by(padding, start);
        }

        write_offset(start + padding);
        commit((std::max)(alignment, needed));
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
        const auto vector = bytes_.size();
        const auto ends = vector + width();
        // The items' ends are written after each item, so they aren't zeroed
        auto* at = bytes_.extend(static_cast<std::size_t>(slots));
        write_offset(vector);
        detail::store_slot(at, count, offset_type_);

        // Once the vector makes its table too long, its other items are
        // written and removed one by one, and only their lengths counted for
        // the error, so that it keeps no more than its table holds and an item
        bool too_long = false;
        std::uint64_t discarded = 0;
        for (std::size_t index = 0; index < count; ++index) {
            if (too_long) [[unlikely]] {
                discarded = detail::saturating_sum(discarded, discard_item(index, fields, header, fill));
                continue;
            }

            const auto item_start = bytes_.size();
            start_in_place(static_cast<std::uint32_t>(fields), header);
            Item item(*this);
            std::invoke(fill, index, item);
            check_filled();
            restore(saved.counters);

            detail::store_slot(bytes_.data() + ends + index * width(), bytes_.size() - vector, offset_type_);
            if (index == 0 && count > 1) {
                reserve_items_like(bytes_.size() - item_start, count);
            }
            too_long = table_length() > max_value(offset_type_);
        }

        check_length(detail::saturating_sum(table_length(), discarded));
    } catch (...) {
        truncate(saved);
        throw;
    }
    commit(Alignment::one);
}

template <class Fill>
std::uint64_t TableBuilder::discard_item(std::size_t index, std::size_t fields, std::uint32_t header, Fill& fill) {
    const auto item = checkpoint();
    start_in_place(static_cast<std::uint32_t>(fields), header);
    Item writer(*this);
    std::invoke(fill, index, writer);
    check_filled();

    const auto length = table_length();
    truncate(item);
    return length;
}

inline TableBuilder::TableBuilder(TableBuilder&& other) noexcept
    : bytes_(std::move(other.bytes_)),
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

inline void TableBuilder::set_table(const NestedTable& table) {
    set_table(table, table.default_alignment());
}

inline void TableBuilder::set_table(const NestedTable& table, Alignment alignment) {
    table.check_complete();
    check_room();

    // Padded from the table's length, unlike an array, since it has no count
    const auto padding = detail::padding(table_length(), alignment);
    const auto length = table.byte_length();
    check_length(detail::saturating_sum(table_length(), detail::saturating_sum(padding, length)));

    const auto start = bytes_.size() + padding;
    auto* at = bytes_.extend(padding + length);
    detail::zero(at, padding);
    write_offset(start);
    table.copy_to(at + padding);
    commit(alignment);
}

inline void TableBuilder::set_vector(std::span<const NestedTable> items) {
    std::uint64_t nested_bytes = 0;
    for (const auto& item : items) {
        item.check_complete();
        nested_bytes = detail::saturating_sum(nested_bytes, item.byte_length());
    }

    const auto slots = detail::vector_bytes(items.size(), width());
    const auto length = detail::saturating_sum(slots, nested_bytes);
    check_field(length);
    // Within the table's length, which `check_field()` limited, so that
    // nothing after this throws
    bytes_.reserve_more(static_cast<std::size_t>(length));

    const auto vector = bytes_.size();
    const auto ends = vector + width();
    // The items' ends are written after each item, so they aren't zeroed
    auto* at = bytes_.extend(static_cast<std::size_t>(slots));
    write_offset(vector);
    detail::store_slot(at, items.size(), offset_type_);

    for (std::size_t index = 0; index < items.size(); ++index) {
        const auto& item = items[index];
        item.copy_to(bytes_.extend(item.byte_length()));
        detail::store_slot(bytes_.data() + ends + index * width(), bytes_.size() - vector, offset_type_);
    }
    commit(Alignment::one);
}

}  // namespace tafla

#endif
