module github.com/snovader/tafla/go/benchmarks

go 1.23

require (
	github.com/google/flatbuffers v25.12.19+incompatible
	github.com/planetscale/vtprotobuf v0.6.0
	github.com/snovader/tafla/go v0.0.0
	github.com/vmihailenco/msgpack/v5 v5.4.1
	google.golang.org/protobuf v1.36.12
)

require github.com/vmihailenco/tagparser/v2 v2.0.0 // indirect

replace github.com/snovader/tafla/go => ../
