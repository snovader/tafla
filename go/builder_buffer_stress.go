//go:build tafla_builder_stress

package tafla

// The sizes of a TableBuilder's buffer, with the build tag
// tafla_builder_stress: it starts empty and grows by the bytes that each
// write needs, so that its tests write every field into new bytes.
const (
	firstCapacity     = 0
	roomForAField     = 0
	mostRoomForFields = 0
	itemRoom          = 0
)

func grownCapacity(int) int { return 0 }
