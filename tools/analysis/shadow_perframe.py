# Fit a flexible per-frame UIShadow model: z = A * Phi(-d / s), d = superellipse distance to a pulled-in rect.
import sys
sys.path.insert(0, "tools/analysis")
from shadow_probe import load
import numpy as np
from scipy.special import ndtr
from scipy.optimize import least_squares

def flex(TX, TY, W, H, P):
    A, px, py, r, e, s = P
    hw = W / 2 - px
    hh = H / 2 - py
    r = max(r, 0.01)
    x0 = np.abs(TX - W / 2) - hw + r
    y0 = np.abs(TY - H / 2) - hh + r
    x1 = np.maximum(x0, 0)
    y1 = np.maximum(y0, 0)
    dpos = (x1 ** e + y1 ** e) ** (1 / e)
    dneg = np.minimum(np.maximum(x0, y0), 0)
    d = dpos + dneg - r
    return A * ndtr(-d / s)

def fit_frame(alpha, f, sigma, margin=2.7, step=1):
    W = f["X1"] - f["X0"]; H = f["Y1"] - f["Y0"]
    M = int(margin * sigma)
    ys, xs = np.mgrid[f["Y0"] - M:f["Y1"] + M:step, f["X0"] - M:f["X1"] + M:step]
    TX = xs + 0.5 - f["X0"]; TY = ys + 0.5 - f["Y0"]
    outside = (TX < 0) | (TY < 0) | (TX > W) | (TY > H)
    v = alpha[ys, xs][outside]
    TX = TX[outside]; TY = TY[outside]
    best = None
    for r0 in [0.5 * sigma, 1.5 * sigma, 2.5 * sigma]:
        for e0 in [2.0, 3.5]:
            try:
                fit = least_squares(lambda P: (flex(TX, TY, W, H, P) - v) * 255, [1.0, 0, 0, r0, e0, sigma],
                                    bounds=([0, -50, -50, 0.01, 1.0, sigma * 0.5], [2, 200, 200, 500, 12, sigma * 2]))
            except Exception as ex:
                continue
            if best is None or fit.cost < best.cost:
                best = fit
    rms = np.sqrt((best.fun ** 2).mean())
    return best.x, rms, np.abs(best.fun).max(), W, H

if __name__ == "__main__":
    run = sys.argv[1] if len(sys.argv) > 1 else "out/compare/shadow-probe-a"
    blurs = [float(b) for b in sys.argv[2].split(",")] if len(sys.argv) > 2 else [20, 40, 80] + [40] * 15
    alpha, frames = load(run)
    for f, blur in zip(frames, blurs):
        sigma = blur * 0.75
        P, rms, mx, W, H = fit_frame(alpha, f, sigma)
        A, px, py, r, e, s = P
        print(f"{f['w']:4d}x{f['h']:<4d} blur {blur:3.0f} (W/s={W/sigma:5.2f} H/s={H/sigma:5.2f}): A={A:.3f} px={px:6.2f} py={py:6.2f} r={r:6.2f} ({r/sigma:.2f}s) e={e:.2f} s={s:.2f} ({s/sigma:.3f})  rms {rms:.2f} max {mx:.1f}")
