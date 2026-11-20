# Tafla.Gen

[tafla-gen](https://github.com/snovader/tafla/tree/main/rust/gen#readme) for
.NET projects: the package generates C# from a project's tafla schemas when
the project builds, and compiles it with the project. The generated code
writes and reads tables with tafla's runtime, the package
[Tafla](https://www.nuget.org/packages/Tafla), which the project references
too.

```xml
<ItemGroup>
    <PackageReference Include="Tafla" Version="…" />
    <PackageReference Include="Tafla.Gen" Version="…" PrivateAssets="all" />
</ItemGroup>

<ItemGroup>
    <TaflaSchema Include="shapes.tafla" />
</ItemGroup>
```

```csharp
using Shapes;

byte[] bytes = new PointFields { X = 1.5, Y = 2, Id = 7, Name = "a" }.Serialize();

var point = new Point(bytes);
string name = point.Name;
```

The project only builds with the package: `PrivateAssets="all"` keeps it from
the projects and packages that depend on the project, and nothing of it is
needed when the project runs. tafla-gen's README describes
[schemas](https://github.com/snovader/tafla/tree/main/rust/gen#schemas) and
[the code that it generates](https://github.com/snovader/tafla/tree/main/rust/gen#generated-code),
and its changelog names the version of Tafla that the generated code needs.

## Schemas

Each `TaflaSchema` item is a schema. Its code's namespace is its file's name
in `PascalCase`, such as `Shapes` for `shapes.tafla`, unless the item names
one:

```xml
<TaflaSchema Include="schemas/*.tafla" Namespace="Example.Schemas" />
```

Schemas of one namespace generate together, so two of them can't generate
the same file, such as `a/shapes.tafla` and `b/shapes.tafla`: the build fails
with tafla-gen's error.

| Property             | Sets                                                                                                    |
| -------------------- | ------------------------------------------------------------------------------------------------------- |
| `TaflaRuntime`       | The namespace that the generated code reads tafla's runtime from, which is `Tafla` by default           |
| `TaflaGenOutputPath` | The directory of the generated code, which is `tafla` in the project's intermediate directory, in `obj` |
| `TaflaGenCommand`    | The path of the tafla-gen command to run, rather than the package's own                                 |

## The build

The code is generated before the project compiles, in an IDE's design-time
builds too, into the project's intermediate directory, so it isn't among the
project's sources, and `dotnet clean` removes it. A build generates every
schema's code again when a schema, the list of schemas, a namespace or the
package's version has changed, and otherwise compiles the code that it has.

A schema's error fails the build. tafla-gen prints the error with the
schema's path in the project, its line and its column, which the build's
output shows below the error.

The package holds a tafla-gen command for Linux, macOS and Windows, on x64
and Arm64, and runs the one of the host that builds the project, whatever the
project targets. Linux's commands are linked statically, and run with glibc
and with musl. On another host, or to generate with another tafla-gen, set
`TaflaGenCommand` to its path, such as the path of the command that
`cargo install tafla-gen` installs.

NuGet extracts a package's files so that the user who restores it can run
them. A build by another user, such as one in a container whose packages an
image's root user restored, can't run the package's command: restore as the
user who builds, or set `TaflaGenCommand`.
