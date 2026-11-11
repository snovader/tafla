package tafla_test

import (
	"flag"
	"os"
	"testing"
)

// TestMain runs 256 cases of each property, as proptest does in the Rust
// tests, unless -rapid.checks asks for another number.
func TestMain(m *testing.M) {
	flag.Parse()

	checks := flag.Lookup("rapid.checks")
	set := false
	flag.Visit(func(f *flag.Flag) { set = set || f == checks })
	if checks != nil && !set {
		_ = checks.Value.Set("256")
	}

	os.Exit(m.Run())
}
