import type { Config } from './config';
import { helper as importedHelper } from './helper';
export { helper as renamedHelper } from './helper';
export * from './types';

interface BaseRunner {
  run(input: string): number;
}

declare class BaseService {}
declare class Worker {
  execute(): number;
}

/** Public runner contract used by the adapter fixture. */
export interface Runner extends BaseRunner {
  run(input: string): number;
}

export type Result = { ok: boolean };

export enum State {
  Ready,
  Failed = 'failed',
}

export class Service extends BaseService implements Runner {
  private readonly prefix: string;

  constructor(prefix: string) {
    super();
    this.prefix = prefix;
  }

  public run(input: string): number {
    importedHelper(input);
    return new Worker().execute();
  }
}

export const makeService = (prefix: string): Service => new Service(prefix);

function internalTask(): Service {
  return makeService('fixture');
}

export namespace Tools {
  export function configure(config: Config): Result {
    return { ok: Boolean(config) };
  }
}

describe('Service', () => {
  it('runs', () => {
    const service = new Service('fixture');
    expect(service.run('value')).toBe(1);
  });
});

void internalTask;
