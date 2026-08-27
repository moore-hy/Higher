/**
 * DEV-0077.3 §六十一：node:test 最小类型 shim（不引入 @types/node /
 * Jest / Vitest——零第三方依赖，只用 Node 内置 test runner）。
 */
declare module "node:test" {
  export function test(name: string, fn: () => void | Promise<void>): void;
  export function describe(name: string, fn: () => void): void;
}
declare module "node:assert/strict" {
  export function ok(value: unknown, message?: string): void;
  export function equal<T>(actual: unknown, expected: T, message?: string): void;
  export function notEqual(actual: unknown, expected: unknown, message?: string): void;
}
