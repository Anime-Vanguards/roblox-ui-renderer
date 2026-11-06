# Round 3 shadow refit: model3 + canonical size offset dc (when c < cmax), floor quantization
import sys; sys.path.insert(0,"tools/analysis")
import numpy as np
import shadow_model3 as M
from scipy.special import ndtr
from scipy.optimize import least_squares
DEF = {**M.DEF, "dc": 1.0}
def shadow(X, Y, W, H, sigma, R=0.0, P=None):
    P = {**DEF, **(P or {})}
    s = sigma * P["ks"]; pad = P["padk"] * sigma
    Reff = min(R, W / 2, H / 2)
    cmax = max(P["capk"] * sigma, 2 * Reff)
    Wc, Hc = min(W, cmax), min(H, cmax)
    c = min(Wc, Hc)
    if c < cmax: c = min(c + P["dc"], cmax)
    corner_cap = cmax / 2 + pad
    u = M.slice_map(X, W / 2 + pad, c, pad, corner_cap)
    v = M.slice_map(Y, H / 2 + pad, c, pad, corner_cap)
    e_base = P["e0"] + P["ke"] * (Wc + Hc) / sigma
    f = min(P["fk"] * Reff / (Wc + Hc), 1.0)
    k = (1 - f) * 2 ** (-1 / e_base) + f / np.sqrt(2)
    e = -np.log(2) / np.log(k)
    A = np.tanh(W / (P["lam"] * sigma)) * np.tanh(H / (P["lam"] * sigma))
    n = (u ** e + v ** e) ** (1 / e)
    return A * ndtr(-(n - c / 2) / s)
def residuals(data, P, q=None):
    out=[]
    for d in data:
        z = shadow(d["X"], d["Y"], d["W"], d["H"], d["sigma"], d["R"], P) * 255
        if q is not None: z = np.floor(z + q)
        else: z = z - 0.5  # floor(z) ~ z - 0.5 on average
        out.append(z - d["v"])
    return out
def report(data,P):
    r=np.concatenate(residuals(data,P,0.0))
    print(f"  rms {np.sqrt((r**2).mean()):.3f} max {abs(r).max():.0f} exact {np.mean(r==0)*100:.2f}% within1 {np.mean(abs(r)<=1)*100:.2f}% within2 {np.mean(abs(r)<=2)*100:.2f}%")
if __name__=="__main__":
    data=M.dataset(); sub=M.dataset(step=5)
    base={k:DEF[k] for k in ["ke","e0","ks","lam","fk"]}
    print("model3 (dc 0):"); report(data,{"dc":0.0})
    print("dc 1:"); report(data,{})
    keys=list(sys.argv[1].split(",")) if len(sys.argv)>1 else ["ke","e0","ks","lam","fk","dc"]
    fit=least_squares(lambda x: np.concatenate(residuals(sub,dict(zip(keys,x)))), [DEF[k] for k in keys], diff_step=1e-3)
    P=dict(zip(keys,fit.x)); print("fit",{k:round(float(v),4) for k,v in P.items()}); report(data,P)
