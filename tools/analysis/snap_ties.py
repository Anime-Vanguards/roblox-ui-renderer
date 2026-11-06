# Readout of fixtures/snap-probe-ties*.luau captures: for every probe frame, whether Studio (and rur) lit pixel
# row / column k of an edge at k + 0.5 - e, from Studio's own AbsolutePosition (studio-nodes.json).
#   python tools/analysis/snap_ties.py out/compare/<dir> [--scale 1]
import json, math, sys
from collections import defaultdict
import numpy as np
from PIL import Image

d = sys.argv[1]
S = float(sys.argv[sys.argv.index('--scale') + 1]) if '--scale' in sys.argv else 1.0
a = np.asarray(Image.open(d + '/studio.png').convert('L')).astype(int)
b = np.asarray(Image.open(d + '/rur/render.png').convert('L')).astype(int)
data = json.load(open(d + '/studio-nodes.json'))
nodes = data['nodes'] if isinstance(data, dict) else data
rows = defaultdict(list)
for n in nodes:
    name = n['path'].split('/')[-1]
    if name[:1] not in 'AB' or '_' not in name:
        continue
    if name[0] == 'A':
        v = n['y'] * S
        k = math.floor(v)
        x = int(n['x'] * S) + 10
        s, o = a[k, x] > 128, b[k, x] > 128
    else:
        v = n['x'] * S
        k = math.floor(v)
        y = int(n['y'] * S) + 20
        s, o = a[y, k] > 128, b[y, k] > 128
    rows[(name[0], k)].append((k + 0.5 - v, v, int(s), int(o)))
mismatch = 0
for key in sorted(rows):
    v = sorted(rows[key])
    mismatch += sum(s != o for e, _, s, o in v)
    print(key, 'studio', ''.join(str(s) for *_, s, o in v), 'rur', ''.join(str(o) for *_, s, o in v), 'e', ' '.join('%.5f' % e for e, *_ in v[::4]))
print('frames', sum(len(v) for v in rows.values()), 'mismatch', mismatch)
