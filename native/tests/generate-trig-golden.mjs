// Frozen renderer math oracle: Node 22 / V8 12.4 fdlibm.
import fs from 'node:fs';
if (!process.versions.v8.startsWith('12.4.')) {
    throw new Error(`The calibrated trig oracle requires V8 12.4; found ${process.versions.v8}`);
}
const storage = new ArrayBuffer(8), view = new DataView(storage);
const bits = x => { view.setFloat64(0, x); return view.getBigUint64(0).toString(16).padStart(16, '0'); };
const float = x => { view.setBigUint64(0, x); return view.getFloat64(0); };
const inputs = [0, -0, NaN, Infinity, -Infinity, Number.MAX_VALUE, -Number.MAX_VALUE, Number.MIN_VALUE, -Number.MIN_VALUE];
for (let degree = -1440; degree <= 1440; degree += 0.5) inputs.push(degree * Math.PI / 180);
for (let exponent = -1074; exponent <= 1023; exponent += 7) {
    for (const mantissa of [1, 1.0000000000000002, 1.23456789012345, 1.9999999999999998]) {
        const x = mantissa * 2 ** exponent;
        inputs.push(x, -x);
    }
}
for (let i = 1; i < 40; i++) {
    const x = i * Math.PI / 2, b = BigInt('0x' + bits(x));
    for (let ulp = -3n; ulp <= 3n; ulp++) inputs.push(float(b + ulp), -float(b + ulp));
}
let state = 0x759ae1943f62b780n;
for (let i = 0; i < 4000; i++) {
    state ^= state << 13n; state = BigInt.asUintN(64, state);
    state ^= state >> 7n; state ^= state << 17n; state = BigInt.asUintN(64, state);
    inputs.push(float(state));
}
fs.writeFileSync(new URL('./goldens/trig-v8.txt', import.meta.url), [...new Map(inputs.map(x => [bits(x), x])).values()]
    .map(x => `${bits(x)} ${bits(Math.sin(x))} ${bits(Math.cos(x))}`).join('\n') + '\n');
