# Per-frame fit of Levien-structured model with free effective parameters.
import sys
sys.path.insert(0, "tools/analysis")
from shadow_probe import load
import numpy as np
from scipy.special import erf
from scipy.optimize import least_squares

def lev(TX, TY, W, H, P):
    scale, band, px, py, r1, e, s = P
    w = W - 2 * px
    h = H - 2 * py
    x0 = r1 + np.abs(TX - W / 2) - 0.5 * w
    y0 = r1 + np.abs(TY - H / 2) - 0.5 * h
    x1 = np.maximum(x0, 0); y1 = np.maximum(y0, 0)
    dpos = (x1 ** e + y1 ** e) ** (1 / e)
    dneg = np.minimum(np.maximum(x0, y0), 0)
    d = dpos + dneg - r1
    return scale * (erf((band + d) / s) - erf(d / s))

if __name__ == "__main__":
    run = sys.argv[1] if len(sys.argv) > 1 else "out/compare/shadow-probe-a"
    blurs = [float(b) for b in sys.argv[2].split(",")] if len(sys.argv) > 2 else [20, 40, 80] + [40] * 15
    alpha, frames = load(run)
    for f, blur in zip(frames, blurs):
        sigma = blur * 0.75
        W = f["X1"] - f["X0"]; H = f["Y1"] - f["Y0"]
        M = int(2.7 * sigma)
        ys, xs = np.mgrid[f["Y0"] - M:f["Y1"] + M, f["X0"] - M:f["X1"] + M]
        TX = xs + 0.5 - f["X0"]; TY = ys + 0.5 - f["Y0"]
        out = (TX < 0) | (TY < 0) | (TX > W) | (TY > H)
        v = alpha[ys, xs][out]; TX = TX[out]; TY = TY[out]
        s0 = sigma * np.sqrt(2)
        best = None
        for r0 in [0.5 * min(W, H), min(2.2 * sigma, 0.5 * min(W, H))]:
            for e0 in [2.0, 3.4, 6.0]:
                P0 = [0.5, min(W, H), 0, 0, r0, e0, s0]
                lo = [0, 1, -20, -20, 0.5, 1.0, s0 * 0.5]
                hi = [1, 2000, 150, 150, 400, 20, s0 * 2]
                try:
                    fit = least_squares(lambda P: (lev(TX, TY, W, H, P) - v) * 255, P0, bounds=(lo, hi))
                except Exception:
                    continue
                if best is None or fit.cost < best.cost:
                    best = fit
        scale, band, px, py, r1, e, s = best.x
        rms = np.sqrt((best.fun ** 2).mean())
        print(f"{f['w']:4d}x{f['h']:<4d} b{blur:3.0f} W/s={W/sigma:5.2f} H/s={H/sigma:5.2f}: scale={scale:.4f} band={band:7.2f} px={px:6.2f} py={py:6.2f} r1={r1:6.2f} ({r1/sigma:.2f}s) e={e:5.2f} s={s/np.sqrt(2)/sigma:.3f}sig  rms {rms:.2f} max {np.abs(best.fun).max():.1f}")
