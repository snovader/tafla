// The RuleTester that the plugin's tests run their cases with, as tests of
// node:test
import { describe, it } from 'node:test';
import { RuleTester } from 'oxlint/plugins-dev';

// RuleTester takes nothing from what these return: node:test's runner awaits
// their promises itself
// oxlint-disable-next-line typescript/no-misused-promises, typescript/strict-void-return -- As above
RuleTester.describe = describe;
// oxlint-disable-next-line typescript/no-misused-promises, typescript/strict-void-return -- As above
RuleTester.it = it;

export const tester = new RuleTester({
    languageOptions: { parserOptions: { lang: 'ts' } }
});
