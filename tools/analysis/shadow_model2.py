# UIShadow model v2: 9-sliced canonical c-square, L_e-ball distance, tanh amplitude.
import sys
sys.path.insert(0, "tools/analysis")
from shadow_model import all_data, frame_data, slice_map, B_BLURS, A_BLURS
import numpy as np
from scipy.special import ndtr

DEF = dict(cap=6.0, pad=3.0, ecap=6.0, e0=2.0, ke=0.2, lam=np.sqrt(np.pi), ks=1.0, q=0.5)

def shadow(X, Y, W, H, sigma, P=None):
    P = {**DEF, **(P or {})}
    cap = P["cap"] * sigma
    c = min(W, H, 2 * (cap - P["pad"] * sigma))
    u = np.abs(slice_map(X, W / 2 + P["pad"] * sigma, c, sigma, cap, P["pad"]))
    v = np.abs(slice_map(Y, H / 2 + P["pad"] * sigma, c, sigma, cap, P["pad"]))
    e = P["e0"] + P["ke"] * (min(W, P["ecap"] * sigma) + min(H, P["ecap"] * sigma)) / sigma
    A = np.tanh(W / (P["lam"] * sigma)) * np.tanh(H / (P["lam"] * sigma))
    n = (u ** e + v ** e) ** (1 / e)
    return A * ndtr(-(n - c / 2) / (P["ks"] * sigma))

def residuals(data, P, quantize=True):
    out = []
    for d in data:
        z = shadow(d["X"], d["Y"], d["W"], d["H"], d["sigma"], P) * 255
        if quantize:
            z = np.floor(z + {**DEF, **(P or {})}["q"])
        out.append(z - d["v"])
    return out

def report(data, P, verbose=True):
    res = residuals(data, P)
    if verbose:
        for d, r in zip(data, res):
            print(f"  {d['name']:16s} rms {np.sqrt((r**2).mean()):5.2f} max {np.abs(r).max():5.1f} exact {np.mean(np.abs(r) < 0.5)*100:5.1f}%")
    allr = np.concatenate(res)
    print(f"  TOTAL rms {np.sqrt((allr**2).mean()):.3f} max {np.abs(allr).max():.1f} exact {np.mean(np.abs(allr) < 0.5)*100:.2f}% within1 {np.mean(np.abs(allr) < 1.5)*100:.2f}%")

if __name__ == "__main__":
    data = all_data(step=1)
    print("default params")
    report(data, {})
    from scipy.optimize import least_squares
    sub = all_data(step=5)
    keys = sys.argv[1].split(",") if len(sys.argv) > 1 else ["ke", "e0", "ks", "lam"]
    fit = least_squares(lambda x: np.concatenate(residuals(sub, dict(zip(keys, x)), quantize=False)), [DEF[k] for k in keys], diff_step=1e-3)
    P = dict(zip(keys, fit.x))
    print("fitted", {k: round(float(v), 4) for k, v in P.items()})
    report(data, P)
