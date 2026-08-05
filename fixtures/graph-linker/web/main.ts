import { helper } from "./helper";

export function run(): number {
  return helper();
}

export function dynamic(factory: () => { run(): number }): number {
  return factory().run();
}
