//go:build !tafla_builder_stress

package tafla

// The sizes of a TableBuilder's buffer. With the build tag
// tafla_builder_stress, it starts empty and grows by the bytes that each
// write needs, so that its tests write every field into new bytes.
const (
	// The least room that new bytes start with, which holds a small table
	// whole
	firstCapacity = 256
	// The bytes that new bytes start with room for, for each field of the
	// table, after its header, up to mostRoomForFields
	roomForAField = 64
	// The most room that new bytes start with for a table's fields
	mostRoomForFields = 64 << 10
	// The most room that a vector's first item makes for the others
	itemRoom = 64 << 10
	// The capacity below which the buffer grows fourfold rather than twofold,
	// which grows a table of many short fields fewer times, and above which it
	// wastes no more than half its capacity
	fourfoldBelow = 64 << 10
)

// grownCapacity returns the capacity that the buffer grows to from capacity,
// at least, which is at least firstCapacity
func grownCapacity(capacity int) int {
	if capacity < fourfoldBelow {
		return max(4*capacity, firstCapacity)
	}

	return 2 * capacity
}
