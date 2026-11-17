// The RuleTester that the plugin's tests run their cases with, as tests of
// node:test
import { RuleTester } from 'oxlint/plugins-dev';
import { describe, it } from 'test-runner';

RuleTester.describe = describe;
RuleTester.it = it;

export const tester = new RuleTester({
    languageOptions: { parserOptions: { lang: 'ts' } }
});
