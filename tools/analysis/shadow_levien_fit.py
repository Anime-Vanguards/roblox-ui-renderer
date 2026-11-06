# Fit a generalized Levien blurred-rect model (free constants) to UIShadow probe captures.
import sys, json
sys.path.insert(0, "tools/analysis")
from shadow_probe import load
import numpy as np
from scipy.special import erf
from scipy.optimize import least_squares

def model(TX, TY, W, H, sigma, P):
    k0, k1, kd, a, c, sig = P
    sigma = sigma * sig
    s = sigma * np.sqrt(2)
    min_edge = min(W, H)
    rmax = 0.5 * min_edge
    r0 = min(k0 * sigma, rmax)
    r1 = min(k1 * sigma, rmax)
    e = 2 * r1 / r0
    delta = kd * sigma * (np.exp(-(0.5 * W / (a * sigma)) ** 2) - np.exp(-(0.5 * H / (a * sigma)) ** 2))
    w = W + min(delta, 0)
    h = H - max(delta, 0)
    scale = 0.5 * erf(c * max(w, h) / sigma)
    x = np.abs(TX - W / 2)
    y = np.abs(TY - H / 2)
    x0 = r1 + x - 0.5 * w
    y0 = r1 + y - 0.5 * h
    x1 = np.maximum(x0, 0)
    y1 = np.maximum(y0, 0)
    dpos = (x1 ** e + y1 ** e) ** (1 / e)
    dneg = np.minimum(np.maximum(x0, y0), 0)
    d = dpos + dneg - r1
    return scale * (erf((min_edge + d) / s) - erf(d / s))

def samples(run, blurs, margin=2.6, step=2):
    alpha, frames = load(run)
    out = []
    for f, blur in zip(frames, blurs):
        sigma = blur * 0.75
        W = f["X1"] - f["X0"]; H = f["Y1"] - f["Y0"]
        M = int(margin * sigma)
        ys, xs = np.mgrid[f["Y0"] - M:f["Y1"] + M:step, f["X0"] - M:f["X1"] + M:step]
        TX = xs + 0.5 - f["X0"]; TY = ys + 0.5 - f["Y0"]
        outside = (TX < 0) | (TY < 0) | (TX > W) | (TY > H)
        out.append(dict(name=f"{f['w']}x{f['h']}@{blur}", W=W, H=H, sigma=sigma, TX=TX[outside], TY=TY[outside], v=alpha[ys, xs][outside]))
    return out

if __name__ == "__main__":
    blurs = [20, 40, 80] + [40] * 15
    data = samples("out/compare/shadow-probe-a", blurs)
    data = [d for d in data if not d["name"].startswith("300x300@80")]
    def resid(P):
        return np.concatenate([(model(d["TX"], d["TY"], d["W"], d["H"], d["sigma"], P) - d["v"]) * 255 for d in data])
    P0 = [1.29, 2.21, 1.25, np.sqrt(2), 0.5 / np.sqrt(2), 1.0]
    fit = least_squares(resid, P0)
    print("params", [round(x, 4) for x in fit.x], "rms", np.sqrt((fit.fun ** 2).mean()))
    for d in data:
        err = (model(d["TX"], d["TY"], d["W"], d["H"], d["sigma"], fit.x) - d["v"]) * 255
        print(f"  {d['name']:14s} rms {np.sqrt((err**2).mean()):6.2f} max {np.abs(err).max():6.2f}")
