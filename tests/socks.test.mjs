import test from 'node:test';
import assert from 'node:assert/strict';
import { parseSocksNodes } from '../src/lib/socks.ts';
test('import authentication, password colons, deduplication and IPv6', () => {
  assert.deepEqual(parseSocksNodes('# comment\r\n192.0.2.1:1080:user:pa:ss\n192.0.2.1:1080:user:pa:ss\n[::1]:1081::'), [
    {host: '192.0.2.1', port: 1080, username: 'user', password: 'pa:ss'},
    {host: '::1', port: 1081, username: '', password: ''},
  ]);
});
test('reject malformed list atomically without printing credentials', () => {
  for (const line of ['', 'host:0:user:secret', 'host:65536:user:secret', 'http://host:1080:u:secret', 'host:1080:'+ '中'.repeat(86) +':secret', 'host:1080:user']) {
    assert.throws(() => parseSocksNodes(line), error => !error.message.includes('secret'));
  }
});
test('limit imported list to 100 nodes', () => {
  assert.throws(() => parseSocksNodes(Array.from({length:101}, (_,i) => `node${i}:1080::`).join('\n')), /100/);
});
