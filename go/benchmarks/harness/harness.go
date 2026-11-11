// Package harness times variants of a benchmark interleaved, as the other
// languages' comparisons do, rather than one after another, as go test
// -bench does.
package harness

import (
	"runtime"
	"slices"
	"time"
)

// Variant is one library's way to do a benchmark's work.
type Variant struct {
	Name string
	// Loop does the work n times, keeping each result alive, in a loop of
	// the variant's own, which calls the library's code directly
	Loop func(n int)
}

// Measurement is a variant's fastest and median call, and what a call
// allocates.
type Measurement struct {
	Name string
	// Fastest is the fastest round's time per call in nanoseconds, and
	// Median the median round's, which load from other processes moves away
	// from it. They're fractions, since a call can take less than a
	// nanosecond, and the time of one rounds to a whole nanosecond.
	Fastest, Median float64
	// Allocations and Bytes are what a call allocates on Go's heap, the
	// mallocs and the bytes they ask for.
	Allocations, Bytes float64
}

// Options set how long each variant runs.
type Options struct {
	Rounds       int
	WarmupRounds int
	// SampleTime is the time that one round of one variant should take.
	SampleTime time.Duration
}

// Measure times the variants interleaved: every round runs each of them
// once, in alternating order, and the fastest round counts. Load from other
// processes then slows every variant alike, and only for some rounds. Each
// round starts after a garbage collection, as go test's benchmarks do, so
// that one variant's garbage doesn't slow another, and the collections that
// a variant's allocations cause within the round count as its own.
//
// A round of a variant repeats its call for about SampleTime. The
// repetitions are counted from the variant's first calls, and then again
// from its fastest warm-up round. Then it counts what each variant's calls
// allocate, apart from the timed rounds.
func Measure(variants []Variant, options Options) []Measurement {
	repetitions := make([]int, len(variants))
	for index, variant := range variants {
		repetitions[index] = calibrate(variant, options.SampleTime)
	}

	if options.WarmupRounds > 0 {
		warm := rounds(variants, repetitions, options.WarmupRounds)
		for index, samples := range warm {
			repetitions[index] = repetitionsOf(slices.Min(samples), options.SampleTime)
		}
	}

	timed := rounds(variants, repetitions, options.Rounds)

	measurements := make([]Measurement, len(variants))
	for index, variant := range variants {
		allocations, bytes := allocated(variant, repetitions[index])
		samples := slices.Clone(timed[index])
		slices.Sort(samples)

		measurements[index] = Measurement{
			Name:        variant.Name,
			Fastest:     samples[0],
			Median:      samples[len(samples)/2],
			Allocations: allocations,
			Bytes:       bytes,
		}
	}

	return measurements
}

// rounds runs rounds of the variants, each once a round, in alternating
// order, and returns each one's time per call in every round, in
// nanoseconds
func rounds(variants []Variant, repetitions []int, count int) [][]float64 {
	samples := make([][]float64, len(variants))

	for round := range count {
		for position := range variants {
			index := position
			if round%2 == 1 {
				index = len(variants) - 1 - position
			}

			samples[index] = append(samples[index], perCall(variants[index], repetitions[index]))
		}
	}

	return samples
}

// perCall returns the time per call of n calls of a variant, in
// nanoseconds, after a garbage collection
func perCall(variant Variant, n int) float64 {
	runtime.GC()

	start := time.Now()
	variant.Loop(n)
	elapsed := time.Since(start)

	return float64(elapsed) / float64(n)
}

// calibrate counts the repetitions that take about sampleTime, from calls
// that take at least a tenth of it
func calibrate(variant Variant, sampleTime time.Duration) int {
	for n := 1; ; n *= 2 {
		start := time.Now()
		variant.Loop(n)
		if elapsed := time.Since(start); elapsed >= sampleTime/10 {
			return repetitionsOf(float64(elapsed)/float64(n), sampleTime)
		}
	}
}

// repetitionsOf returns the calls of perCall nanoseconds that take about
// sampleTime
func repetitionsOf(perCall float64, sampleTime time.Duration) int {
	return max(1, int(float64(sampleTime)/max(perCall, 1)))
}

// allocated returns the mallocs and bytes per call of n calls, the fewest of
// a few samples
func allocated(variant Variant, n int) (allocations, bytes float64) {
	allocations, bytes = -1, -1
	var before, after runtime.MemStats

	for range 3 {
		runtime.GC()
		runtime.ReadMemStats(&before)
		variant.Loop(n)
		runtime.ReadMemStats(&after)

		sampleAllocations := float64(after.Mallocs-before.Mallocs) / float64(n)
		sampleBytes := float64(after.TotalAlloc-before.TotalAlloc) / float64(n)
		if allocations < 0 || sampleAllocations < allocations {
			allocations, bytes = sampleAllocations, sampleBytes
		}
	}

	return allocations, bytes
}
