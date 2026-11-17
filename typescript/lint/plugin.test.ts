import plugin from './plugin.ts';
import { tester } from './ruleTester.ts';

const MESSAGE =
    /^Expected "[^"]+" before "[^"]+": names in lower case, then upper case, then types, each in natural order$/u;

tester.run('sort-named-imports', plugin.rules['sort-named-imports'], {
    valid: [
        "import { alpha, beta, Alpha, SIZEOF_8, SIZEOF_16, _private, type Zed } from 'a';",
        "import Default, { a, b } from 'a';",
        "import * as all from 'a';",
        "import type { Reader, Writer } from 'a';",
        "import { a as y, b as z } from 'a';",
        "import {\n    a,\n    // a comment\n    B\n} from 'a';"
    ],
    invalid: [
        {
            name: 'names in lower case come before names in upper case',
            code: "import { Upper, lower } from 'a';",
            output: "import { lower, Upper } from 'a';",
            errors: [{ message: MESSAGE }]
        },
        {
            name: 'types come last',
            code: "import { type Zed, Alpha, alpha } from 'a';",
            output: "import { alpha, Alpha, type Zed } from 'a';",
            errors: [{ message: MESSAGE }]
        },
        {
            name: 'numbers are in natural order',
            code: "import { SIZEOF_16, SIZEOF_8 } from 'a';",
            output: "import { SIZEOF_8, SIZEOF_16 } from 'a';",
            errors: [{ message: MESSAGE }]
        },
        {
            name: 'names of a type import are in natural order',
            code: "import type { Writer, Reader } from 'a';",
            output: "import type { Reader, Writer } from 'a';",
            errors: [{ message: MESSAGE }]
        },
        {
            name: 'aliases are sorted by their local names',
            code: "import { a as z, b as y } from 'a';",
            output: "import { b as y, a as z } from 'a';",
            errors: [{ message: MESSAGE }]
        },
        {
            name: 'names on several lines keep their lines',
            code: "import Default, {\n    beta,\n    alpha\n} from 'a';",
            output: "import Default, {\n    alpha,\n    beta\n} from 'a';",
            errors: [{ message: MESSAGE }]
        },
        {
            name: 'names with comments between them are reported but not moved',
            code: "import {\n    B,\n    // a comment\n    a\n} from 'a';",
            output: null,
            errors: [{ message: MESSAGE }]
        }
    ]
});
