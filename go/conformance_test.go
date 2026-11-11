package tafla_test

// Reads and rebuilds the tables of tafla's conformance suite, which the
// TypeScript runtime writes. See conformance/README.md at the root of the
// repository for the format of their descriptions in cases.json.

import (
	"bytes"
	"errors"
	"testing"

	tafla "github.com/snovader/tafla/go"
	"github.com/snovader/tafla/go/internal/tables"
)

func loadSuite(t *testing.T) *tables.Suite {
	t.Helper()

	suite, err := tables.LoadSuite()
	if err != nil {
		t.Fatal(err)
	}

	return suite
}

func TestConformanceReadsEveryValidTable(t *testing.T) {
	for _, valid := range loadSuite(t).Valid {
		read, err := tables.ReadTable(valid.Bytes, valid.Table)
		if err != nil {
			t.Errorf("%s: %v", valid.Name, err)
		} else if !tables.Equal(read, valid.Table) {
			t.Errorf("%s: read %+v\nrather than %+v", valid.Name, read, valid.Table)
		}
	}
}

func TestConformanceRebuildsEveryValidTableByteForByte(t *testing.T) {
	for _, valid := range loadSuite(t).Valid {
		built, err := valid.Table.Bytes()
		if err != nil {
			t.Errorf("%s: %v", valid.Name, err)
		} else if !bytes.Equal(built, valid.Bytes) {
			t.Errorf("%s: built % x\nrather than % x", valid.Name, built, valid.Bytes)
		}
	}
}

// Reads every equivalent table, whose bytes differ from its valid case's, as
// the valid case's description, header included
func TestConformanceReadsEquivalentTablesAsTheirValidCase(t *testing.T) {
	suite := loadSuite(t)

	for _, equivalent := range suite.Equivalent {
		table := suite.TableOf(equivalent.Case)
		read, err := tables.ReadTable(equivalent.Bytes, table)
		if err != nil {
			t.Errorf("%s: %v", equivalent.Name, err)
		} else if !tables.Equal(read, table) {
			t.Errorf("%s: read %+v\nrather than %+v", equivalent.Name, read, table)
		}
	}
}

// Reads the root fields before the invalid case's field, which must hold
// their described values, and then the field, which must fail with a decode
// error
func TestConformanceReadsInvalidTablesUpToTheFieldThatFails(t *testing.T) {
	suite := loadSuite(t)

	for _, invalid := range suite.Invalid {
		table := suite.TableOf(invalid.Case)

		for index, expected := range table.Fields[:invalid.Field] {
			read, err := tables.ReadRootField(invalid.Bytes, table.Layout, index, expected)
			if err != nil || !tables.EqualFields(read, expected) {
				t.Errorf("%s/%d: read %+v, %v rather than %+v", invalid.Name, index, read, err, expected)
			}
		}

		field := table.Fields[invalid.Field]
		_, err := tables.ReadRootField(invalid.Bytes, table.Layout, invalid.Field, field)
		var decodeError *tafla.DecodeError
		if !errors.As(err, &decodeError) {
			t.Errorf("%s/%d: %v rather than a decode error", invalid.Name, invalid.Field, err)
		}
	}
}
