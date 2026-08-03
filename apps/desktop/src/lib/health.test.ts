import { describe, expect, it, vi } from 'vitest';
import {
  CURRENT_PROTOCOL_VERSION,
  parseHealthResponseV1,
  queryHealth,
  type HealthResponseV1,
} from './health';

const validResponse: HealthResponseV1 = {
  applicationVersion: '0.1.0',
  platform: 'windows',
  protocolVersion: CURRENT_PROTOCOL_VERSION,
  status: 'ready',
};

describe('health IPC client', () => {
  it('sends the current versioned request and validates the response', async () => {
    const invokeCommand = vi.fn(async () => validResponse);

    await expect(queryHealth(invokeCommand)).resolves.toEqual(validResponse);
    expect(invokeCommand).toHaveBeenCalledWith('query_health', {
      request: { protocolVersion: CURRENT_PROTOCOL_VERSION },
    });
  });

  it('rejects an unknown response protocol version', () => {
    expect(() =>
      parseHealthResponseV1({
        ...validResponse,
        protocolVersion: 2,
      }),
    ).toThrowError('unsupported protocol version');
  });

  it('rejects additional fields from an untrusted response', () => {
    expect(() =>
      parseHealthResponseV1({
        ...validResponse,
        unexpected: true,
      }),
    ).toThrowError('does not match the V1 schema');
  });
});
