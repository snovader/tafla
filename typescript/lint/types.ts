// The types of oxlint's plugin API, which oxlint names only in RuleTester's
import type { RuleTester } from 'oxlint/plugins-dev';

export type Rule = Parameters<RuleTester['run']>[1];
export type Context = Parameters<NonNullable<Rule['create']>>[0];
export type Visitor = ReturnType<NonNullable<Rule['create']>>;
export type Report = Parameters<Context['report']>[0];
export type Fix = NonNullable<Report['fix']>;

/** The node that a visitor of `Type` nodes takes */
export type NodeOf<Type extends keyof Visitor> = Parameters<
    NonNullable<Visitor[Type]>
>[0];
