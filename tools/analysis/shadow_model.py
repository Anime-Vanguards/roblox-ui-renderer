# Candidate UIShadow model: 9-sliced canonical square shadow (superellipse SDF + Gaussian falloff).
import sys, json
sys.path.insert(0, "tools/analysis")
from shadow_probe import load
import numpy as np
from scipy.special import ndtr

DEFAULT = dict(k1=2 * np.sqrt(2), ke=0.8, e0=2.0, lam=np.sqrt(np.pi), ks=1.0, cap=6.0, pad=3.0, q=0.5)

def sdf(u, v, half, r, e):
    x0 = np.abs(u) - half + r
    y0 = np.abs(v) - half + r
    x1 = np.maximum(x0, 0); y1 = np.maximum(y0, 0)
    return (x1 ** e + y1 ** e) ** (1 / e) + np.minimum(np.maximum(x0, y0), 0) - r

def slice_map(X, half_screen, c, sigma, cap, pad):
    Th = c / 2 + pad * sigma
    Sc = np.minimum(half_screen, cap)
    S = Sc / Th
    a = np.abs(X)
    u = Th - (half_screen - a) / S
    u = np.where(a >= half_screen - Sc, u, 0)
    return np.sign(X) * np.maximum(u, 0)

def shadow(X, Y, W, H, sigma, P=None):
    P = {**DEFAULT, **(P or {})}
    cap = P["cap"] * sigma
    c = min(W, H, cap - 2 * (P["pad"] - 3.0) * sigma) if False else min(W, H, 2 * (cap - P["pad"] * sigma))
    r1 = min(P["k1"] * sigma, c / 2)
    e = P["e0"] + P["ke"] * r1 / sigma
    A = np.tanh(W / (P["lam"] * sigma)) * np.tanh(H / (P["lam"] * sigma))
    u = slice_map(X, W / 2 + P["pad"] * sigma, c, sigma, cap, P["pad"])
    v = slice_map(Y, H / 2 + P["pad"] * sigma, c, sigma, cap, P["pad"])
    return A * ndtr(-sdf(u, v, c / 2, r1, e) / (sigma * P["ks"]))

def frame_data(run, blurs, clear, skip=()):
    alpha, frames = load(run)
    out = []
    for i, (f, blur) in enumerate(zip(frames, blurs)):
        if i in skip:
            continue
        sigma = blur * 0.75
        W = f["X1"] - f["X0"]; H = f["Y1"] - f["Y0"]
        M = int(3.2 * sigma)
        y0, y1 = max(f["Y0"] - M, 0), min(f["Y1"] + M, alpha.shape[0])
        x0, x1 = max(f["X0"] - M, 0), min(f["X1"] + M, alpha.shape[1])
        ys, xs = np.mgrid[y0:y1, x0:x1]
        X = xs + 0.5 - (f["X0"] + f["X1"]) / 2
        Y = ys + 0.5 - (f["Y0"] + f["Y1"]) / 2
        mask = np.ones(X.shape, dtype=bool) if clear else ~((np.abs(X) < W / 2) & (np.abs(Y) < H / 2))
        out.append(dict(name=f"{f['w']}x{f['h']}b{blur:g}", W=W, H=H, sigma=sigma, X=X[mask], Y=Y[mask], v=alpha[ys, xs][mask] * 255))
    return out

def residuals(data, P, quantize=True):
    res = []
    for d in data:
        z = shadow(d["X"], d["Y"], d["W"], d["H"], d["sigma"], P) * 255
        if quantize:
            z = np.floor(z + P.get("q", 0.5))
        res.append(z - d["v"])
    return res

def report(data, P):
    res = residuals(data, P)
    for d, r in zip(data, res):
        print(f"  {d['name']:16s} rms {np.sqrt((r**2).mean()):5.2f} max {np.abs(r).max():5.1f} exact {np.mean(np.abs(r) < 0.5)*100:5.1f}%")
    allr = np.concatenate(res)
    print(f"  TOTAL rms {np.sqrt((allr**2).mean()):.3f} max {np.abs(allr).max():.1f} exact {np.mean(np.abs(allr) < 0.5)*100:.2f}%")

A_BLURS = [20, 40, 80] + [40] * 15
B_BLURS = [40] * 9 + [20, 20, 20] + [40] * 4 + [10, 4, 120, 120, 120]

def all_data(step=1):
    data = frame_data("out/compare/shadow-probe-a-clear", A_BLURS, True, skip=(2,))
    data += frame_data("out/compare/shadow-probe-b-clear", B_BLURS[:12], True)
    if step > 1:
        for d in data:
            for k in ("X", "Y", "v"):
                d[k] = d[k][::step]
    return data

if __name__ == "__main__":
    from scipy.optimize import least_squares
    data = all_data(step=7)
    keys = ["k1", "ke", "e0", "lam", "ks", "cap", "pad"]
    def f(vec):
        P = dict(zip(keys, vec))
        return np.concatenate([r for r in residuals(data, P, quantize=False)]) - 0.0
    x0 = [DEFAULT[k] for k in keys]
    fit = least_squares(f, x0, diff_step=1e-3)
    P = dict(zip(keys, fit.x))
    print({k: round(v, 4) for k, v in P.items()})
    report(all_data(step=1), P)
