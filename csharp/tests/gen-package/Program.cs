// Writes and reads tables with the code that the package Tafla.Gen generated from the project's
// schemas when it built, as scripts/test-gen-package.sh runs it

using Example.Labels;
using Shapes;

byte[] documentBytes = new DocumentFields
{
    X = 1.5,
    Y = 2,
    Name = "a",
    Columns = new() { Base = 0, Keys = new uint[] { 1, 2 }, A = new[] { 0.5f, 1.5f }, B = new[] { 2.5f, 3.5f } },
}.Serialize();

var document = new Document(documentBytes);
if (document.Name != "a" || document.Columns.A.ToArray() is not [0.5f, 1.5f])
{
    Console.Error.WriteLine($"Read back \"{document.Name}\" and [{string.Join(", ", document.Columns.A.ToArray())}]");
    return 1;
}

byte[] labelBytes = new LabelFields { Id = 7, Name = "seven" }.Serialize();

var label = new Label(labelBytes);
if (label.Id != 7 || label.Name != "seven")
{
    Console.Error.WriteLine($"Read back {label.Id} and \"{label.Name}\"");
    return 1;
}

Console.WriteLine("Code that Tafla.Gen generated wrote and read a document and a label");
return 0;
