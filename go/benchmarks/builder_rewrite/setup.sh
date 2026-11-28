#!/bin/sh
# Builds the command that times main's TableBuilder, #216's and this
# branch's, with variants of this branch's, as the stacked PR's tables were
# timed. Each runtime is copied into a package of its own, with the rows of
# templates/ generated into it, so that every builder compiles alike, and
# the A/A twins are copies of their own, so that their difference shows code
# placement too. Run it from the root of a checkout of the branch, with
# main's and #216's commits fetched:
#
#   sh go/benchmarks/builder_rewrite/setup.sh <output directory> [main] [#216]
#
# Then run <output directory>/builder-timing under the benchmark lock, in
# several processes, each with -rotate set to another number.
set -eu
out=$1
main=${2:-630f0016}
pr216=${3:-89e75d4f}
tree=$(pwd)
templates=$tree/go/benchmarks/builder_rewrite/templates

rm -rf "$out/go"
mkdir -p "$out"
cp -R "$tree/go" "$out/go"
packages=$out/go/benchmarks/builder_rewrite/builders
rm -rf "$packages"

# copy_runtime copies the runtime's files at a commit, or the branch's, into
# the package of name
copy_runtime() {
	name=$1 source=$2 dir=$packages/$1
	mkdir -p "$dir"
	if [ "$source" = branch ]; then
		files=$(cd "$tree/go" && ls *.go | grep -v '_test\.go$')
	else
		files=$(git -C "$tree" ls-tree --name-only "$source" go/ | sed 's|^go/||' | grep '\.go$' | grep -v '_test\.go$')
	fi
	for file in $files; do
		if [ "$source" = branch ]; then
			cp "$tree/go/$file" "$dir/$file"
		else
			git -C "$tree" show "$source:go/$file" >"$dir/$file"
		fi
		sed -i '' "s/^package tafla$/package $name/" "$dir/$file"
	done
}

# add_template writes a template into the package of name, unqualified,
# with ROOT and FINISH(builder) replaced by root and finish, which take
# builder in \1
add_template() {
	name=$1 template=$2 root=$3 finish=$4
	sed -e "s/^package template$/package $name/" \
		-e '/tafla "github.com\/snovader\/tafla\/go"$/d' \
		-e 's/tafla\.//g' \
		-e "s/ROOT(/$root(/g" \
		-e "s/FINISH(\(.*\))$/$finish/" \
		"$templates/$template.go.tmpl" >"$packages/$name/$template.go"
}

list=""
imports=""
# add_builder adds a builder of label, in the package of name, to the
# command's list
add_builder() {
	name=$1 label=$2
	imports="$imports	\"github.com/snovader/tafla/go/benchmarks/builder_rewrite/builders/$name\"
"
	list="$list		{\"$label\", func() []row { return rowsOf($name.Rows(), func(r $name.Row) row { return row{r.Name, r.Call} }) }},
"
}

plain='\1.Bytes()'
for spec in "mainline:$main:main" "mainline_aa:$main:main A\/A" "pr216:$pr216:#216"; do
	name=${spec%%:*} rest=${spec#*:}
	source=${rest%%:*} label=${rest#*:}
	copy_runtime "$name" "$source"
	for template in serialize shapes hooks_fresh; do
		add_template "$name" $template NewTableBuilder "$plain"
	done
	add_builder "$name" "$label"
done

for spec in "buffer:buffer" "buffer_aa:buffer A\/A"; do
	name=${spec%%:*} label=${spec#*:}
	copy_runtime "$name" branch
	for template in serialize shapes hooks_room; do
		add_template "$name" $template NewTableBuilder "$plain"
	done
	add_builder "$name" "$label"
done

copy_runtime clone branch
for template in serialize shapes hooks_room hooks_clone; do
	add_template clone $template NewTableBuilder 'cloned(\1.Bytes())'
done
add_builder clone "buffer, bytes.Clone"

copy_runtime reused branch
for template in serialize shapes hooks_fresh hooks_reuse; do
	add_template reused $template reuse 'keep(\1.Bytes())'
done
add_builder reused "buffer, reused"

copy_runtime coalloc branch
sed -i '' 's/^func NewTableBuilder(/func newTableBuilder(/' "$packages/coalloc/table_builder.go"
for template in serialize shapes hooks_room coalloc_api; do
	add_template coalloc $template NewTableBuilder "$plain"
done
add_builder coalloc "co-allocated"

for spec in "grow2:grows twofold" "appendgrow:grows by append" "roomfields:room by fields"; do
	name=${spec%%:*} label=${spec#*:}
	copy_runtime "$name" branch
	python3 "$templates/variants.py" "$packages/$name" "$name"
	for template in serialize shapes hooks_room; do
		add_template "$name" $template NewTableBuilder "$plain"
	done
	add_builder "$name" "$label"
done

copy_runtime declared branch
add_template declared declared_rows NewTableBuilder "$plain"
add_builder declared "declared alignment"

copy_runtime beginend branch
for template in beginend_api beginend_rows; do
	add_template beginend $template NewTableBuilder "$plain"
done
add_builder beginend "begin\/end"

mkdir -p "$out/go/benchmarks/builder_rewrite/cmd"
printf '%s' "$imports" >"$out/imports"
printf '%s' "$list" >"$out/list"
sed -e "/^\/\/ IMPORTS$/r $out/imports" -e '/^\/\/ IMPORTS$/d' \
	-e "/^\/\/ BUILDERS$/r $out/list" -e '/^\/\/ BUILDERS$/d' \
	"$templates/main.go.tmpl" >"$out/go/benchmarks/builder_rewrite/cmd/main.go"
sed -i '' 's/\\\//\//g' "$out/go/benchmarks/builder_rewrite/cmd/main.go"

cd "$out/go/benchmarks"
gofmt -w builder_rewrite
go vet ./builder_rewrite/...
go build -o "$out/builder-timing" ./builder_rewrite/cmd
echo "Built $out/builder-timing"
