# UIShadow model v3: radius-aware 9-slice of a canonical L_e-ball shadow.
import sys
sys.path.insert(0, "tools/analysis")
from shadow_model import frame_data, A_BLURS, B_BLURS
import numpy as np
from scipy.special import ndtr

DEF = dict(capk=6.0, padk=3.0, e0=2.0, ke=0.2, lam=np.sqrt(np.pi), ks=1.0, q=0.5, fk=4.0)

def slice_map(X, half_screen, c, pad, corner_cap):
    Th = c / 2 + pad
    Sc = np.minimum(half_screen, corner_cap)
    S = Sc / Th
    a = np.abs(X)
    u = Th - (half_screen - a) / S
    return np.where(a >= half_screen - Sc, np.maximum(u, 0), 0)

def shadow(X, Y, W, H, sigma, R=0.0, P=None):
    P = {**DEF, **(P or {})}
    s = sigma * P["ks"]
    pad = P["padk"] * sigma
    Reff = min(R, W / 2, H / 2)
    cmax = max(P["capk"] * sigma, 2 * Reff)
    Wc, Hc = min(W, cmax), min(H, cmax)
    c = min(Wc, Hc)
    corner_cap = cmax / 2 + pad
    u = slice_map(X, W / 2 + pad, c, pad, corner_cap)
    v = slice_map(Y, H / 2 + pad, c, pad, corner_cap)
    e_base = P["e0"] + P["ke"] * (Wc + Hc) / sigma
    f = min(P["fk"] * Reff / (Wc + Hc), 1.0)
    k = (1 - f) * 2 ** (-1 / e_base) + f / np.sqrt(2)
    e = -np.log(2) / np.log(k)
    A = np.tanh(W / (P["lam"] * sigma)) * np.tanh(H / (P["lam"] * sigma))
    n = (u ** e + v ** e) ** (1 / e)
    return A * ndtr(-(n - c / 2) / s)

C_CELLS = [(30, R) for R in [0, 4, 8, 15, 22.5, 30, 45, 60]] + [(30, R) for R in [0, 5, 10, 15, 20]] + [(30, R) for R in [0, 15, 30]] \
    + [(16, R) for R in [0, 6, 12, 25]] + [(16, R) for R in [8, 16, 40]] + [(60, 20), (60, 60), (30, 30), (30, 60), (12, 50)]

def dataset(step=1):
    data = []
    for d in frame_data("out/compare/shadow-probe-a-clear", A_BLURS, True, skip=(2,)):
        d["R"] = 0.0
        data.append(d)
    for d in frame_data("out/compare/shadow-probe-b-clear", B_BLURS[:12], True):
        d["R"] = 0.0
        data.append(d)
    for d, (b, R) in zip(frame_data("out/compare/shadow-probe-c-clear", [b for b, R in C_CELLS], True), C_CELLS):
        d["R"] = R * 1.5
        d["name"] += f"R{R:g}"
        data.append(d)
    if step > 1:
        for d in data:
            for k in ("X", "Y", "v"):
                d[k] = d[k][::step]
    return data

def residuals(data, P=None, quantize=True):
    PP = {**DEF, **(P or {})}
    out = []
    for d in data:
        z = shadow(d["X"], d["Y"], d["W"], d["H"], d["sigma"], d["R"], P) * 255
        if quantize:
            z = np.floor(z + PP["q"])
        out.append(z - d["v"])
    return out

def report(data, P=None, verbose=True):
    res = residuals(data, P)
    if verbose:
        for d, r in zip(data, res):
            print(f"  {d['name']:20s} rms {np.sqrt((r**2).mean()):5.2f} max {np.abs(r).max():5.1f} exact {np.mean(np.abs(r) < 0.5)*100:5.1f}%")
    allr = np.concatenate(res)
    print(f"  TOTAL rms {np.sqrt((allr**2).mean()):.3f} max {np.abs(allr).max():.1f} exact {np.mean(np.abs(allr) < 0.5)*100:.2f}% within1 {np.mean(np.abs(allr) < 1.5)*100:.2f}% within2 {np.mean(np.abs(allr) < 2.5)*100:.2f}%")

if __name__ == "__main__":
    data = dataset()
    print("defaults")
    report(data, {}, verbose="-v" in sys.argv)
    if "--fit" in sys.argv:
        from scipy.optimize import least_squares
        sub = dataset(step=6)
        keys = ["ke", "e0", "ks", "lam", "fk"]
        fit = least_squares(lambda x: np.concatenate(residuals(sub, dict(zip(keys, x)), quantize=False)), [DEF[k] for k in keys], diff_step=1e-3)
        P = dict(zip(keys, fit.x))
        print("fitted", {k: round(float(v), 4) for k, v in P.items()})
        report(data, P, verbose=True)
