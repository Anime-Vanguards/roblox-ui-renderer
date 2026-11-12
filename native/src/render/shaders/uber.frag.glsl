// The uber-shader's fragment stage. This file (with uber.vert.glsl) is the hand-maintained source, not generated.
// Every draw is a record in the RGBA32F data texture (layout above each kind, written by packet.rs and the draw
// builders); the shader reproduces one measured operation per record kind. Exactness: the GPU's output
// conversion truncates a float to 12 bits and rounds half down to 8 bits (conv12), then blends
// round(s + d (1 - sa / 255)). Where Studio's result is integer arithmetic (premultiplied bytes, masks, the
// image blend) the shader redoes that arithmetic and outputs bytes / 255; where it is a float (gradients,
// resampled quads) it outputs the same float (notes/gpu.md, "Measured Canvas2D arithmetic"). Keep the
// calibrated arithmetic and its evaluation order as they are.
#version 450
layout(set=1,binding=12) uniform sampler uPointSampler;

layout(set=1,binding=0) uniform texture2D uData;
layout(set=1,binding=1) uniform texture2D uGlyphs;
layout(set=1,binding=2) uniform texture2D uLayer;
layout(set=1,binding=3) uniform texture2D uLut;
layout(set=1,binding=4) uniform texture2D uImage0;
layout(set=1,binding=5) uniform texture2D uImage1;
layout(set=1,binding=6) uniform texture2D uImage2;
layout(set=1,binding=7) uniform texture2D uImage3;
layout(set=1,binding=8) uniform texture2D uImage4;
layout(set=1,binding=9) uniform texture2D uImage5;
layout(set=1,binding=10) uniform texture2D uImage6;
layout(set=1,binding=11) uniform texture2D uImage7;

layout(location=0) flat in int vRecord;
layout(location=1) in vec2 vGradient;
layout(location=2) in vec2 vUv;
layout(set=1,binding=13) uniform sampler uRepeatSampler;
layout(location=0) out vec4 outColor;

// HLSL (ANGLE/D3D11) rejects discard inside helper functions: helpers flag it and main discards
bool gDiscard = false;
// The paint (paintFloat) and image UIGradient factors (imageGradient) at this pixel, evaluated once in main:
// FXC inlines every call, and a gradient pipeline per call site made the program compile 5x slower
vec4 gPaint = vec4(0.0);
vec4 gImageGradient = vec4(1.0);
#define DISCARD { gDiscard = true; return vec4(0.0); }

vec4 rec(int i) {
	int k = vRecord + i;
	return texelFetch(sampler2D(uData, uPointSampler), ivec2(k & 2047, k >> 11), 0);
}

vec4 imageTexel(int slot, ivec2 p, int lod) {
	switch (slot) {
		case 0: return texelFetch(sampler2D(uImage0, uPointSampler), p, lod);
		case 1: return texelFetch(sampler2D(uImage1, uPointSampler), p, lod);
		case 2: return texelFetch(sampler2D(uImage2, uPointSampler), p, lod);
		case 3: return texelFetch(sampler2D(uImage3, uPointSampler), p, lod);
		case 4: return texelFetch(sampler2D(uImage4, uPointSampler), p, lod);
		case 5: return texelFetch(sampler2D(uImage5, uPointSampler), p, lod);
		case 6: return texelFetch(sampler2D(uImage6, uPointSampler), p, lod);
		case 7: return texelFetch(sampler2D(uImage7, uPointSampler), p, lod);
		// The batch's layer: a CanvasGroup's texture drawn as a turned quad (groups.rs composite_turned_group)
		case 8: return texelFetch(sampler2D(uLayer, uPointSampler), p, lod);
	}
	return vec4(0.0);
}

ivec2 imageSize(int slot, int lod) {
	switch (slot) {
		case 0: return textureSize(sampler2D(uImage0, uPointSampler), lod);
		case 1: return textureSize(sampler2D(uImage1, uPointSampler), lod);
		case 2: return textureSize(sampler2D(uImage2, uPointSampler), lod);
		case 3: return textureSize(sampler2D(uImage3, uPointSampler), lod);
		case 4: return textureSize(sampler2D(uImage4, uPointSampler), lod);
		case 5: return textureSize(sampler2D(uImage5, uPointSampler), lod);
		case 6: return textureSize(sampler2D(uImage6, uPointSampler), lod);
		case 7: return textureSize(sampler2D(uImage7, uPointSampler), lod);
		case 8: return textureSize(sampler2D(uLayer, uPointSampler), lod);
	}
	return ivec2(1);
}

// float32(k / 255) for a byte k: data texels 0-63 hold them (packet.rs byte_float_texels)
float byteFloat(int k) {
	vec4 v = texelFetch(sampler2D(uData, uPointSampler), ivec2(k >> 2, 0), 0);
	return v[k & 3];
}

// Integer helpers (bytes are exact integers)

int mulDiv255(int a, int b) {
	return (a * b + 127) / 255;
}

ivec4 mulDiv255(ivec4 a, int b) {
	return (a * b + 127) / 255;
}

int toByte(float v) {
	return int(floor(v * 255.0 + 0.5));
}

ivec4 toBytes(vec4 v) {
	return ivec4(floor(v * 255.0 + 0.5));
}

vec4 fromBytes(ivec4 c) {
	return vec4(c) / 255.0;
}

// floor(v * 255 + 0.5) of a float32 v evaluated exactly, as float64 gives it (v * 255 never rounds there):
// the CPU side stores float32 values and quantizes them in float64
int byteOf(float v) {
	if (!(v > 0.0)) {
		return 0;
	}
	if (v >= 1.0) {
		return 255;
	}
	uint bits = floatBitsToUint(v);
	int exponent = int((bits >> 23) & 255u) - 127;
	if (exponent < -127 + 1) {
		return 0;
	}
	uint mantissa = (bits & 0x7FFFFFu) | 0x800000u;
	int shift = 23 - exponent;
	uint product = mantissa * 255u;
	if (shift > 32) {
		return 0;
	}
	if (shift == 32) {
		return product >= 0x80000000u ? 1 : 0;
	}
	uint q = product >> uint(shift);
	uint rest = product - (q << uint(shift));
	return int(q) + (rest >= (1u << uint(shift - 1)) ? 1 : 0);
}

ivec4 bytesOf(vec4 v) {
	return ivec4(byteOf(v.r), byteOf(v.g), byteOf(v.b), byteOf(v.a));
}

// The hardware's float -> unorm8 conversion (truncate to 12 bits, round half down to 8 bits)
int conv12(float x) {
	int t = int(floor(clamp(x, 0.0, 1.0) * 4096.0));
	return (t * 255 + 2047) >> 12;
}

ivec4 conv12(vec4 c) {
	return ivec4(conv12(c.r), conv12(c.g), conv12(c.b), conv12(c.a));
}

// gpu8(q * f) of float32 q, f in [0, 1] multiplied in double (exact): floor(q f 4096) from their 24-bit
// mantissas in 12-bit limbs, then the GPU's 12-bit to 8-bit rounding
int gpu8Mul(float q, float f) {
	if (!(q > 0.0) || !(f > 0.0)) {
		return 0;
	}
	uint bq = floatBitsToUint(q), bf = floatBitsToUint(f);
	int eq = int(bq >> 23) - 127, ef = int(bf >> 23) - 127;
	uint mq = (bq & 0x7FFFFFu) | 0x800000u, mf = (bf & 0x7FFFFFu) | 0x800000u;
	uint a1 = mq >> 12, a0 = mq & 0xFFFu, b1 = mf >> 12, b0 = mf & 0xFFFu;
	uint mid = a1 * b0 + a0 * b1;
	// floor(mq mf / 2^24), then / 2^(shift - 24)
	uint x = a1 * b1 + ((mid + ((a0 * b0) >> 12)) >> 12);
	int shift = 34 - eq - ef - 24;
	uint t12 = shift >= 32 ? 0u : (shift <= 0 ? x << uint(-shift) : x >> uint(shift));
	return int((min(t12, 4096u) * 255u + 2047u) >> 12);
}

// gpu8(q * t / 255) of a float32 q in [0, 1] and a tint byte t (the CPU model multiplies in double):
// floor(q * t * 4096 / 255) exactly, then the GPU's 12-bit to 8-bit rounding
int gpu8Tinted(float q, int t) {
	if (!(q > 0.0) || t == 0) {
		return 0;
	}
	uint b = floatBitsToUint(q);
	int e = int(b >> 23) - 127;
	uint n = ((b & 0x7FFFFFu) | 0x800000u) * uint(t);
	int shift = 11 - e;
	int t12 = shift >= 32 ? 0 : int((n / 255u) >> uint(shift));
	return (min(t12, 4096) * 255 + 2047) >> 12;
}

ivec3 unpackRgb(float packed) {
	int p = int(packed);
	return ivec3((p >> 16) & 255, (p >> 8) & 255, p & 255);
}

// Lookup rows: 0-255 the unpremultiply u(P = x, A = y) = round(P * 255 / A) (gpu.rs default_lut_bytes; the
// retired browser renderer measured getImageData's table instead); dynamic rows above hold per-value tables
int lut(int row, int x) {
	return toByte(texelFetch(sampler2D(uLut, uPointSampler), ivec2(x, row), 0).r);
}

ivec3 lut3(int row, ivec3 x) {
	return ivec3(toByte(texelFetch(sampler2D(uLut, uPointSampler), ivec2(x.r, row), 0).r), toByte(texelFetch(sampler2D(uLut, uPointSampler), ivec2(x.g, row), 0).g), toByte(texelFetch(sampler2D(uLut, uPointSampler), ivec2(x.b, row), 0).b));
}

ivec3 unpremul(ivec3 c, int a) {
	return ivec3(lut(a, c.r), lut(a, c.g), lut(a, c.b));
}

// Gradients, as Chrome 153's Skia (Ganesh) shades a canvas linear gradient (GrGradientShader.cpp; the
// CPU half is paint.rs skia_linear_gradient):
//   - LinearLayout: t = coord.x + 0.00001, coord = the float32 pts-to-unit matrix times the local
//     coordinate (interpolated from the quad's vertices when the record is the gradient's own quad)
//   - ClampedGradient: t < 0 -> left border colour, t > 1 -> right border colour (premultiplied)
//   - colorizer (unpremultiplied): single interval mix(start, end, t), dual interval or the looping
//     binary search over the interval ends, colour = t * scale + bias
//   - make_interpolated_to_dst: colour.rgb *= colour.a unless every stop is opaque
//   - dither: the 8x8 table byte (read through the texture unit like Skia's A8 table) - 0.5, times
//     1/255, added and clamped to [0, alpha], at the pixel of the canvas painted into
// Block: written by paint.rs push_gradient_block.

const float DITHER_RANGE = 0.003921568859368563;

vec4 colorizer(int g, int kind, int n, float t) {
	if (kind == 1) {
		return mix(rec(g + 7), rec(g + 8), t);
	}
	if (kind == 2) {
		vec4 s, b;
		if (t < rec(g + 11).x) {
			s = rec(g + 7);
			b = rec(g + 9);
		} else {
			s = rec(g + 8);
			b = rec(g + 10);
		}
		return t * s + b;
	}
	// Looping binary colorizer: n intervals (a power of two >= 4), thresholds in fours from g + 7
	int chunks = n >> 2;
	int low = 0, high = chunks - 1, chunk = (chunks - 1) / 2;
	for (int loop = 0; loop < 8; loop++) {
		if ((1 << loop) >= chunks) {
			break;
		}
		if (t < rec(g + 7 + chunk).w) {
			high = chunk;
		} else {
			low = chunk + 1;
		}
		chunk = (low + high) / 2;
	}
	vec4 th = rec(g + 7 + chunk);
	int pos;
	if (t < th.y) {
		pos = t < th.x ? 0 : 1;
	} else {
		pos = t < th.z ? 2 : 3;
	}
	pos += 4 * chunk;
	return t * rec(g + 7 + chunks + pos) + rec(g + 7 + chunks + n + pos);
}

vec4 gradientColor(int g, vec2 X, vec2 ditherOrigin) {
	vec4 h = rec(g);
	int kind = int(h.x);
	if (kind == 0) {
		return vec4(0.0);
	}
	int flags = int(h.y);
	float coord;
	if ((flags & 2) != 0) {
		coord = vGradient.x;
	} else {
		vec4 m = rec(g + 1), tr = rec(g + 2), l0 = rec(g + 3), l1 = rec(g + 4);
		vec2 local = vec2(l0.x * X.x + l0.z * X.y + l1.x, l0.y * X.x + l0.w * X.y + l1.y);
		coord = (mat3x2(m.x, m.y, m.z, m.w, tr.x, tr.y) * vec3(local, 1.0)).x;
	}
	float t = coord + 0.00001;
	vec4 color;
	if (t < 0.0) {
		color = rec(g + 5);
	} else if (t > 1.0) {
		color = rec(g + 6);
	} else {
		color = colorizer(g, kind, int(h.z), t);
		if ((flags & 1) == 0) {
			color.rgb *= color.a;
		}
	}
	ivec2 cell = ivec2(floor(X - ditherOrigin)) & 7;
	float value = texelFetch(sampler2D(uLut, uPointSampler), ivec2(cell.x + 8 * cell.y, 256), 0).r - 0.5;
	return vec4(clamp(color.rgb + value * DITHER_RANGE, 0.0, color.a), color.a);
}

// Paint blocks (2 texels, paint.rs Paint::write, which writes modes 0 and 1): mode 0 = solid premultiplied bytes (0, r, g, b),
// (a8, 0, 0, 0); mode 1 = gradient (1, block offset, 0, 0), (0, dither origin, 0); mode 2 = gradient
// layer (2, block offset, atlas x, atlas y), (atlas image slot, layer origin, 0): the bytes the gradient
// left in its layer, rasterized from the layer's origin (GRADIENT_FILL) into the gradient atlas; an
// atlas x of -1 (no room) evaluates the gradient instead.

ivec4 layerGradientBytes(vec4 p0, vec4 p1, vec2 X) {
	ivec2 q = ivec2(floor(X - p1.yz)) + ivec2(p0.zw);
	return toBytes(imageTexel(int(p1.x), q, 0));
}

vec4 paintFloat(int p, vec2 X) {
	vec4 p0 = rec(p), p1 = rec(p + 1);
	if (p0.x < 0.5) {
		return vec4(p0.yzw, p1.x) / 255.0;
	}
	if (p0.x > 1.5 && p0.z >= 0.0) {
		return vec4(layerGradientBytes(p0, p1, X)) / 255.0;
	}
	return gradientColor(int(p0.y), X, p1.yz);
}

// The paint as a canvas layer stores it (8-bit premultiplied) is conv12(paintFloat): exact bytes / 255
// convert back to the same bytes

// Distance helpers

float sdBox(vec2 p, vec2 halfSize) {
	vec2 q = abs(p) - halfSize;
	return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0);
}

float quadrantRadius(vec2 p, vec4 radii) {
	return p.y < 0.0 ? (p.x < 0.0 ? radii.x : radii.y) : (p.x < 0.0 ? radii.w : radii.z);
}

// Signed distance of local pixel (i, j) of a w x h mask to its rounded rect (per-quadrant radius)
float maskDistance(vec2 ij, vec2 halfSize, vec4 radii) {
	vec2 p = ij + 0.5 - halfSize;
	float r = quadrantRadius(p, radii);
	vec2 q = abs(p) - halfSize + r;
	return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

// UICorner mask byte of target pixel P for the snapped rect (x, y, w, h), coarse 2x2 derivatives with
// quads on even target coordinates (fwidth from each quad's top-left pixel; notes/strokes.md)
int cornerMask(ivec2 P, vec4 rect, vec4 radii) {
	vec2 halfSize = rect.zw * 0.5;
	vec2 ij = vec2(P) - rect.xy;
	vec2 q = vec2(P - (P & 1)) - rect.xy;
	vec2 qx = q + vec2(1.0, 0.0), qy = q + vec2(0.0, 1.0);
	float origin = maskDistance(q, halfSize, radii);
	float dx = maskDistance(qx, halfSize, radii), dy = maskDistance(qy, halfSize, radii);
	float fw = abs(dx - origin) + abs(dy - origin);
	// Three of the four pixels of a quad are one of its derivative samples: reuse identical inputs
	float d = ij == q ? origin : ij == qx ? dx : ij == qy ? dy : maskDistance(ij, halfSize, radii);
	float t = fw > 0.0 ? clamp(0.5 - d / fw, 0.0, 1.0) : (d < 0.0 ? 1.0 : 0.0);
	return int(floor(t * t * (3.0 - 2.0 * t) * 255.0 + 0.5));
}

// Kinds

// RECT: (1, hole, 0, 0), hole rect (L, T, R, B), paint
vec4 kindRect(ivec2 P, vec2 X) {
	vec4 h = rec(2);
	if (h.y > 0.5) {
		vec4 hole = rec(3);
		if (X.x >= hole.x && X.x < hole.z && X.y >= hole.y && X.y < hole.w) {
			DISCARD
		}
	}
	return gPaint;
}

// ROUNDED: rect (x, y, w, h), radii (tl, tr, br, bl), paint (the layer's)
vec4 kindRounded(ivec2 P, vec2 X) {
	int m = cornerMask(P, rec(3), rec(4));
	if (m == 0) {
		DISCARD
	}
	ivec4 layer = conv12(gPaint);
	return fromBytes(mulDiv255(layer, m));
}

// ROTATED (fillTransformedRect): inverse (a, b, c, d), (e, f, ox, oy) to rect-centred local space,
// (hw, hh, hole, flags: 1 quarter turn, 2 rounded), radii, quad screen bounds, hole screen bounds, paint
vec2 toLocal(vec4 m, vec4 t, vec2 X) {
	vec2 d = X - t.zw;
	return vec2(m.x * d.x + m.z * d.y + t.x, m.y * d.x + m.w * d.y + t.y);
}

// Coverage of a turned plain quad at its centred local point p: one pixel's ramp past each edge, the nearer
// edge's alone past a corner
float turnedQuad(vec2 p, vec2 halfSize) {
	vec2 q = abs(p) - halfSize;
	return clamp(1.0 - max(q.x, q.y), 0.0, 1.0);
}

// Signed distance of the rect-centred local point p to the rounded rect (per-quadrant radius)
float turnedDistance(vec2 p, vec2 halfSize, vec4 radii) {
	float r = quadrantRadius(p, radii);
	vec2 q = abs(p) - halfSize + r;
	return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

// Whether pixel P's centre is on the inner side of the edge from a to b of a quad whose corners run clockwise
// on the screen, all in 1/256 px: the sign of the edge function, exactly (its products take 40 bits: the high
// and low ten bits of the pixel's offset are multiplied apart), a centre on the edge counting for a top or a
// left edge as the rasterizer counts it
bool gridEdge(ivec2 a, ivec2 b, ivec2 c) {
	ivec2 e = b - a, d = c - a;
	ivec2 dh = d >> 10, dl = d & 1023;
	int high = e.x * dh.y - e.y * dh.x, low = e.x * dl.y - e.y * dl.x;
	high += low >> 10;
	low &= 1023;
	if (high != 0) {
		return high > 0;
	}
	if (low != 0) {
		return true;
	}
	return (e.y == 0 && e.x > 0) || e.y < 0;
}

// Whether pixel P's centre is in the quad whose grid corners are the two texels at `at` (top left and top
// right, then bottom right and bottom left)
bool gridCorners(vec4 top, vec4 bottom, ivec2 P) {
	ivec2 tl = ivec2(top.xy), tr = ivec2(top.zw), br = ivec2(bottom.xy), bl = ivec2(bottom.zw);
	ivec2 c = P * 256 + 128;
	return gridEdge(tl, tr, c) && gridEdge(tr, br, c) && gridEdge(br, bl, c) && gridEdge(bl, tl, c);
}

bool gridQuad(int at, ivec2 P) {
	vec4 top = rec(at), bottom = rec(at + 1);
	ivec2 tl = ivec2(top.xy), tr = ivec2(top.zw), br = ivec2(bottom.xy), bl = ivec2(bottom.zw);
	ivec2 c = P * 256 + 128;
	return gridEdge(tl, tr, c) && gridEdge(tr, br, c) && gridEdge(br, bl, c) && gridEdge(bl, tl, c);
}

vec4 kindRotated(ivec2 P, vec2 X) {
	vec4 m = rec(3), tr = rec(4), shape = rec(5);
	vec2 p = toLocal(m, tr, X);
	vec2 halfSize = shape.xy;
	float hole = shape.z;
	int flags = int(shape.w);
	bool quarter = (flags & 1) != 0, rounded = (flags & 2) != 0;
	// Flag 4: a UIGradient's fill or border. Studio draws a turned one without anti-aliased edges: the pixels
	// whose centre lies in the quad (fixtures/group-probe-border.luau PlainTurnBanded)
	bool hard = (flags & 4) != 0;
	// Flag 8: texels 7 and 8 hold the quad's corners on the 1/256 px grid (gridQuad), not the quarter turn's extents
	bool exact = (flags & 8) != 0;
	vec4 screen = rec(7);
	float a;
	if (quarter && !(X.x >= screen.x && X.x < screen.z && X.y >= screen.y && X.y < screen.w)) {
		a = 0.0;
	} else if (quarter && !rounded) {
		a = 1.0;
	} else if (hard) {
		a = (exact ? gridQuad(7, P) : p.x >= -halfSize.x && p.x < halfSize.x && p.y >= -halfSize.y && p.y < halfSize.y) ? 1.0 : 0.0;
	} else if (!rounded) {
		// Past a corner the coverage follows the nearer edge's ramp alone (a mitred corner), not the distance
		// to the corner point
		vec2 q = abs(p) - halfSize;
		a = clamp(1.0 - max(q.x, q.y), 0.0, 1.0);
	} else if (!quarter && (exact ? !gridQuad(7, P) : p.x < -halfSize.x || p.x >= halfSize.x || p.y < -halfSize.y || p.y >= halfSize.y)) {
		a = 0.0;
	} else {
		// The distance's change over a pixel is the GPU's coarse derivative, as under no rotation
		// (cornerMask): differences within the 2x2 quad on even target coordinates, from its top-left pixel
		vec4 radii = rec(6);
		vec2 Q = X - vec2(P & 1);
		float d = turnedDistance(p, halfSize, radii);
		float origin = turnedDistance(toLocal(m, tr, Q), halfSize, radii);
		float dx = turnedDistance(toLocal(m, tr, Q + vec2(1.0, 0.0)), halfSize, radii);
		float dy = turnedDistance(toLocal(m, tr, Q + vec2(0.0, 1.0)), halfSize, radii);
		float fw = abs(dx - origin) + abs(dy - origin);
		float t = fw > 0.0 ? clamp(0.5 - d / fw, 0.0, 1.0) : (d < 0.0 ? 1.0 : 0.0);
		a = t * t * (3.0 - 2.0 * t);
	}
	if (hole > 0.0 && !rounded && a > 0.0) {
		if (quarter) {
			vec4 inner = rec(8);
			if (X.x >= inner.x && X.x < inner.z && X.y >= inner.y && X.y < inner.w) {
				a = 0.0;
			}
		} else if (hard) {
			int strips = int(rec(6).x);
			if (strips > 0) {
				// Four quads, each on its own grid corners: a pixel centre on the seam of two of them, or of
				// one and the fill, can be in both or in neither
				a = gridQuad(strips, P) || gridQuad(strips + 2, P) || gridQuad(strips + 4, P) || gridQuad(strips + 6, P) ? 1.0 : 0.0;
			} else {
				vec2 inner = halfSize - hole;
				if (p.x >= -inner.x && p.x < inner.x && p.y >= -inner.y && p.y < inner.y) {
					a = 0.0;
				}
			}
		} else {
			// A border is four strips, each a turned quad with its own edge ramps, laid like a pinwheel: the
			// left one holds the top left corner, the top one the top right, the right one the bottom right
			// and the bottom one the bottom left. Each reaches one pixel's ramp past its edges, into the fill
			// and into its neighbours, and what two of them cover in part is covered by both
			// (fixtures/rotation-probe-border.luau).
			float b = hole * 0.5;
			vec2 across = vec2(halfSize.x - b, b), along = vec2(b, halfSize.y - b);
			float top = turnedQuad(p - vec2(b, b - halfSize.y), across), bottom = turnedQuad(p - vec2(-b, halfSize.y - b), across);
			float left = turnedQuad(p - vec2(b - halfSize.x, -b), along), right = turnedQuad(p - vec2(halfSize.x - b, b), along);
			// Each strip is a layer of the border's own opacity: two of them over one another are more
			// opaque than one (a border under BackgroundTransparency 0.5 is 0.75 where they cross)
			float opacity = gPaint.a;
			if (opacity > 0.0) {
				float through = (1.0 - opacity * top) * (1.0 - opacity * bottom) * (1.0 - opacity * left) * (1.0 - opacity * right);
				a = (1.0 - through) / opacity;
			} else {
				a = 0.0;
			}
		}
	}
	int mask = int(floor(a * 255.0 + 0.5));
	if (mask == 0) {
		DISCARD
	}
	ivec4 layer = conv12(gPaint);
	return fromBytes(mulDiv255(layer, mask));
}

// SHADOW: (cx, cy, lut row, rgb), (hx, hy, sigma, Di), per quadrant (r, segments, inner hx, inner hy) or hard
// (r, hw, hh, 0), (vertex alpha, 0, 0, 0). Blurred: Studio's shadow mesh (coverage.rs shadow_mesh / ShadowCorner /
// shadow_alpha8; the lut row holds the 256-texel profile), hard: the rounded rect through the floor(k * alpha)
// row. Straight (rgb, a) put into a layer: premultiplied round(c * a / 255).
float shadowCross(vec2 a, vec2 b) {
	return a.x * b.y - a.y * b.x;
}

vec2 shadowArc(vec2 centre, float radius, int j, float stepAngle) {
	float a = float(j) * stepAngle;
	return centre + radius * vec2(cos(a), sin(a));
}

// D at p (outward coordinates, >= 0) in one corner quadrant: s = (hx, hy, sigma, Di), q = (r, segments, ihx, ihy)
float shadowMeshD(vec2 p, vec4 s, vec4 q) {
	float k = 3.0 * s.z, Di = s.w;
	float r = clamp(q.x, 0.0, min(s.x, s.y));
	int N = int(q.y);
	float ro = r + k, ri = max(r - k, 0.0);
	vec2 C = s.xy - r, CI = q.zw - ri;
	float stepAngle = 1.5707963267948966 / float(N);
	vec2 o0 = shadowArc(C, ro, 0, stepAngle), i0 = ri > 0.0 ? shadowArc(CI, ri, 0, stepAngle) : q.zw;
	if (shadowCross(o0 - i0, p - i0) < 0.0) {
		return p.x <= q.z ? Di : -3.0 + (Di + 3.0) * (s.x + k - p.x) / (s.x + k - q.z);
	}
	vec2 oN = shadowArc(C, ro, N, stepAngle), iN = ri > 0.0 ? shadowArc(CI, ri, N, stepAngle) : q.zw;
	if (shadowCross(oN - iN, p - iN) > 0.0) {
		return p.y <= q.w ? Di : -3.0 + (Di + 3.0) * (s.y + k - p.y) / (s.y + k - q.w);
	}
	// The segment whose spokes (inner vertex -> outer vertex) enclose p
	int lo = 0, hi = N - 1;
	for (int it = 0; it < 6; it++) {
		if (lo >= hi) {
			break;
		}
		int mid = (lo + hi + 1) >> 1;
		vec2 om = shadowArc(C, ro, mid, stepAngle), im = ri > 0.0 ? shadowArc(CI, ri, mid, stepAngle) : q.zw;
		if (shadowCross(om - im, p - im) >= 0.0) {
			lo = mid;
		} else {
			hi = mid - 1;
		}
	}
	vec2 oa = shadowArc(C, ro, lo, stepAngle), ob = shadowArc(C, ro, lo + 1, stepAngle);
	vec2 ia = ri > 0.0 ? shadowArc(CI, ri, lo, stepAngle) : q.zw, ib = ri > 0.0 ? shadowArc(CI, ri, lo + 1, stepAngle) : q.zw;
	if (ri > 0.0) {
		if (shadowCross(ib - ia, p - ia) >= 0.0) {
			return Di;
		}
		float side = shadowCross(ia - ob, p - ob), sideO = shadowCross(ia - ob, oa - ob);
		if ((side >= 0.0) != (sideO >= 0.0)) {
			return Di - (Di + 3.0) * shadowCross(ib - p, ia - p) / shadowCross(ib - ob, ia - ob);
		}
	}
	return -3.0 + (Di + 3.0) * shadowCross(ob - oa, p - oa) / shadowCross(ob - oa, ia - oa);
}

// Alpha byte: round(255 trunc12(profile(D) * va / 255)), profile bilinear with 8-bit weights over the lut row
int shadowAlpha8(int row, float D, int va) {
	float c = clamp((clamp(D, -3.0, 3.0) + 3.0) / 6.0 * 256.0 - 0.5, 0.0, 255.0);
	int i = min(int(floor(c)), 254);
	int w = int(floor((c - float(i)) * 256.0 + 0.5));
	uint n = uint((lut(row, i) * (256 - w) + lut(row, i + 1) * w) * va);
	uint t12 = (n * 256u) / 1040400u;
	return int((255u * t12 + 2048u) >> 12);
}

// Alpha byte of the shadow layer at p (relative to the shadow centre)
int shadowAlpha(vec2 p) {
	vec4 h = rec(3), s = rec(4);
	int quadrant = p.y < 0.0 ? (p.x < 0.0 ? 0 : 1) : (p.x < 0.0 ? 3 : 2);
	vec4 q = rec(5 + quadrant);
	if (s.z > 0.0) {
		return shadowAlpha8(int(h.z), shadowMeshD(abs(p), s, q), int(rec(9).x));
	}
	vec2 halfSize = q.yz;
	if (p.x < -halfSize.x || p.x >= halfSize.x || p.y < -halfSize.y || p.y >= halfSize.y) {
		return 0;
	}
	float r = q.x;
	if (r > 0.0) {
		vec2 c = abs(p) - (halfSize - r);
		if (c.x > 0.0 && c.y > 0.0 && c.x * c.x + c.y * c.y > r * r) {
			return 0;
		}
	}
	return lut(int(h.z), 255);
}

ivec4 shadowTexel(int a) {
	ivec3 rgb = unpackRgb(rec(3).w);
	return ivec4(mulDiv255(ivec4(rgb, 255), a).rgb, a);
}

// Texels of the CPU layers drawn through a rotation (like drawImage under a transform): kind 2 =
// shadow layer
ivec4 layerTexel(int kind, ivec2 ij) {
	vec4 h = rec(3);
	int a = shadowAlpha(vec2(ij) + 0.5 - h.xy);
	return a > 0 ? shadowTexel(a) : ivec4(0);
}

// Bilinear sample of a layer drawn through a transform (Chrome: bilinear, clamp to the image, 8-bit
// weights and 12-bit truncation like every filtered sample on this GPU) with the quad's anti-aliased
// edge coverage. Transform block at rec(tr): inverse, (e, f, ox, oy) to layer pixels, (w, h, 0, 0).
vec4 sampleLayer(int kind, vec2 X, int tr, out float coverage) {
	vec4 m = rec(tr), t = rec(tr + 1), size = rec(tr + 2);
	vec2 l = toLocal(m, t, X);
	// Skia's anti-aliased quad edges: clamp(1/2 + d / fwidth(d)) per edge (L1 fwidth in device pixels),
	// the minimum over the four edges (fitted against Chrome's Canvas2D rotated image layers, the reference
	// renderer's path for rotated layers)
	float du = min(l.x, size.x - l.x) / (abs(m.x) + abs(m.z));
	float dv = min(l.y, size.y - l.y) / (abs(m.y) + abs(m.w));
	coverage = clamp(min(du, dv) + 0.5, 0.0, 1.0);
	if (coverage <= 0.0) {
		return vec4(0.0);
	}
	vec2 s = l - 0.5;
	ivec2 base = ivec2(floor(s));
	ivec2 w = ivec2(floor((s - floor(s)) * 256.0 + 0.5));
	if (w.x == 256) {
		base.x += 1;
		w.x = 0;
	}
	if (w.y == 256) {
		base.y += 1;
		w.y = 0;
	}
	ivec2 last = ivec2(size.xy) - 1;
	ivec2 i0 = clamp(base, ivec2(0), last), i1 = clamp(base + 1, ivec2(0), last);
	ivec4 a = layerTexel(kind, i0);
	if (w.x == 0 && w.y == 0) {
		return vec4(a) / 255.0;
	}
	ivec4 b = w.x > 0 ? layerTexel(kind, ivec2(i1.x, i0.y)) : ivec4(0);
	ivec4 c = w.y > 0 ? layerTexel(kind, ivec2(i0.x, i1.y)) : ivec4(0);
	ivec4 d = w.x > 0 && w.y > 0 ? layerTexel(kind, i1) : ivec4(0);
	ivec4 top = a * (256 - w.x) + b * w.x;
	ivec4 bottom = c * (256 - w.x) + d * w.x;
	ivec4 V = top * (256 - w.y) + bottom * w.y;
	return vec4(V / 4080) / 4096.0;
}

vec4 kindShadow(ivec2 P, vec2 X) {
	vec4 h = rec(3), k = rec(2);
	if (k.y > 0.5) {
		float coverage;
		vec4 value = sampleLayer(2, X, int(k.z), coverage);
		if (coverage <= 0.0 || value.a <= 0.0) {
			DISCARD
		}
		return value * coverage;
	}
	int a = shadowAlpha(X - h.xy);
	if (a <= 0) {
		DISCARD
	}
	// The exact part (scene_draw.rs exact_shadow_boxes): the straight colour for the "image" batch blend
	if (k.w > 0.5) {
		return fromBytes(ivec4(unpackRgb(h.w), a));
	}
	return fromBytes(shadowTexel(a));
}

// STROKE (borderStrokeAlpha): rect edges (X0, Y0, X1, Y1) in local space, radii, (D, I, S, flags: 1 round,
// 2 bevel, 4 inner), inverse (a, b, c, d), (e, f, ox, oy) to local space, 4 quadrant (mode 0 clip /
// 1 ring / 2 hard, margin, R, legs), (interior, alpha, rgb, vertex alpha byte alpha8), (gradient?, 0, 0, 0),
// gradient paint (source-in over the stroke layer)
float strokeOuterDistance(vec2 l, vec2 c, vec2 halfSize, float D, float R) {
	vec2 q = abs(l - c) - halfSize - D + R;
	return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - R;
}

vec4 kindStroke(ivec2 P, vec2 X) {
	vec4 edges = rec(3), radii = rec(4), prm = rec(5), m = rec(6), tr = rec(7);
	float D = prm.x, I = prm.y, S = prm.z;
	int flags = int(prm.w);
	bool roundJoin = (flags & 1) != 0, bevel = (flags & 2) != 0, inner = (flags & 4) != 0;
	vec2 l = toLocal(m, tr, X);
	float x = l.x, y = l.y;
	vec2 halfSize = (edges.zw - edges.xy) * 0.5;
	vec2 c = edges.xy + halfSize;
	vec4 style = rec(12);
	float interior = style.x;
	float v = min(y - edges.y, edges.w - y);
	if (v > interior && min(x - edges.x, edges.z - x) > interior) {
		DISCARD
	}
	bool left = x < c.x;
	int qi = y < c.y ? (left ? 0 : 1) : (left ? 3 : 2);
	vec4 q = rec(8 + qi);
	float r = qi == 0 ? radii.x : qi == 1 ? radii.y : qi == 2 ? radii.z : radii.w;
	bool sharp = r == 0.0;
	int mode = int(q.x);
	float margin = q.y;
	if (x < edges.x - D - margin || x >= edges.z + D + margin || y < edges.y - D - margin || y >= edges.w + D + margin) {
		DISCARD
	}
	float u = min(x - edges.x, edges.z - x);
	if (bevel && D > 0.0) {
		float s = u + v + 2.0 * D - q.w + margin * sqrt(2.0);
		if (s < 0.0 || (s == 0.0 && !left)) {
			DISCARD
		}
	}
	float a;
	if (mode == 1) {
		vec2 qq = abs(l - c) - halfSize + r;
		float d = length(max(qq, 0.0)) + min(max(qq.x, qq.y), 0.0);
		a = clamp(S * (d - (r - I) + 0.5), 0.0, 1.0);
		if (roundJoin) {
			a *= clamp(S * (r + D + 0.5 - d), 0.0, 1.0);
		}
	} else {
		bool hole;
		if (inner && sharp) {
			hole = x > edges.x + I && x <= edges.z - I && y > edges.y + I && y <= edges.w - I;
		} else {
			hole = x >= edges.x + I && x < edges.z - I && y >= edges.y + I && y < edges.w - I;
			if (hole && bevel && inner) {
				float s = u + v - 3.0 * I;
				hole = s > 0.0 || (s == 0.0 && left);
			}
		}
		if (hole) {
			DISCARD
		}
		if (mode == 2) {
			a = 1.0;
		} else {
			vec2 Q = vec2(P - (P & 1)) + 0.5, Qx = Q + vec2(1.0, 0.0), Qy = Q + vec2(0.0, 1.0);
			float d00 = strokeOuterDistance(toLocal(m, tr, Q), c, halfSize, D, q.z);
			float d10 = strokeOuterDistance(toLocal(m, tr, Qx), c, halfSize, D, q.z);
			float d01 = strokeOuterDistance(toLocal(m, tr, Qy), c, halfSize, D, q.z);
			float fw = abs(d10 - d00) + abs(d01 - d00);
			// X is one of the quad's derivative samples for three of its four pixels
			float dX = X == Q ? d00 : X == Qx ? d10 : X == Qy ? d01 : strokeOuterDistance(l, c, halfSize, D, q.z);
			float t = clamp(0.5 - dX / max(fw, 1e-9), 0.0, 1.0);
			a = t * t * (3.0 - 2.0 * t);
		}
	}
	// coverage.rs stroke_alpha_byte: style.w is the vertex alpha byte (alpha8); ring-mode pixels convert
	// coverage * alpha through the GPU's 12-bit path, clip / hard ones truncate
	int full = int(style.w);
	int a8 = a >= 1.0 ? full : mode == 1 ? gpu8Tinted(a, full) : int(floor(a * float(full) + 1e-6));
	if (a8 <= 0) {
		DISCARD
	}
	if (rec(13).x > 0.5) {
		ivec4 g = conv12(gPaint);
		return fromBytes(mulDiv255(g, a8));
	}
	ivec3 rgb = unpackRgb(style.z);
	return fromBytes(ivec4(mulDiv255(ivec4(rgb, 255), a8).rgb, a8));
}

// Image sampling (as sampling.rs sample_quad_region): taps per column / row are precomputed on the CPU (exact
// float64 geometry), the bilinear and trilinear arithmetic runs on integers (8-bit weights, 12-bit
// truncation floor(v / 255 * 4096)).
ivec4 fetchBytes(int slot, ivec2 p, int lod) {
	return toBytes(imageTexel(slot, p, lod));
}

// Bilinear value in 1/65536 units per channel; exact is set when both weights are 0
ivec4 bilinear(int slot, int lod, vec4 tx, vec4 ty, out bool exact) {
	int wx = int(tx.z), wy = int(ty.z);
	ivec4 a = fetchBytes(slot, ivec2(tx.x, ty.x), lod);
	// A zero weight multiplies its texels by 0: skip fetching them
	ivec4 b = wx > 0 ? fetchBytes(slot, ivec2(tx.y, ty.x), lod) : ivec4(0);
	ivec4 c = wy > 0 ? fetchBytes(slot, ivec2(tx.x, ty.y), lod) : ivec4(0);
	ivec4 d = wx > 0 && wy > 0 ? fetchBytes(slot, ivec2(tx.y, ty.y), lod) : ivec4(0);
	ivec4 top = a * (256 - wx) + b * wx;
	ivec4 bottom = c * (256 - wx) + d * wx;
	exact = wx == 0 && wy == 0;
	return top * (256 - wy) + bottom * wy;
}

// Straight colour and alpha of an image pixel before quantization (the sampled texel, as images.rs shade takes it)
// The texel the hardware's point sampling picks at the interpolated coordinate (uber.vert.glsl vUv)
vec4 hardwareTexel(int slot, vec2 uv, float lod) {
	// One sampler for every image (the GL backend takes a texture with a single sampler): it repeats, which is
	// the tile's addressing and changes nothing else, a snapped quad's pixel centres lying half a pixel or
	// more inside its edges
	switch (slot) {
		case 0: return textureLod(sampler2D(uImage0, uRepeatSampler), uv, lod);
		case 1: return textureLod(sampler2D(uImage1, uRepeatSampler), uv, lod);
		case 2: return textureLod(sampler2D(uImage2, uRepeatSampler), uv, lod);
		case 3: return textureLod(sampler2D(uImage3, uRepeatSampler), uv, lod);
	}
	return vec4(0.0);
}

vec4 sampleImageQuad(ivec2 P, out bool ok) {
	vec4 quad = rec(3), info = rec(5), taps = rec(6);
	int slot = int(info.x), l0 = int(info.y), f = int(info.z), flags = int(info.w);
	ivec2 ij = P - ivec2(quad.xy);
	ok = true;
	if ((flags & 1) != 0) {
		vec4 head = rec(2);
		if (head.y > 0.0) {
			// A quad that is not a tile (head.z = 0) clamps its coordinate to the texture's last texel, as the tap
			// tables do: a slice whose middle runs past its texture (SliceScale) would wrap under the sampler
			vec2 uv = vUv;
			if (head.z < 0.5) {
				uv = clamp(uv, vec2(0.0), 1.0 - 0.5 / vec2(imageSize(slot, l0)));
			}
			ivec4 k = toBytes(hardwareTexel(slot, uv, float(l0)));
			return vec4(byteFloat(k.r), byteFloat(k.g), byteFloat(k.b), byteFloat(k.a));
		}
		vec4 tx = rec(int(taps.x) + ij.x), ty = rec(int(taps.y) + ij.y);
		ivec4 k = toBytes(imageTexel(slot, ivec2(tx.x, ty.x), l0));
		return vec4(byteFloat(k.r), byteFloat(k.g), byteFloat(k.b), byteFloat(k.a));
	}
	vec4 tx0 = rec(int(taps.x) + ij.x), ty0 = rec(int(taps.y) + ij.y);
	bool exact;
	ivec4 v0 = bilinear(slot, l0, tx0, ty0, exact);
	if (f > 0) {
		vec4 tx1 = rec(int(taps.z) + ij.x), ty1 = rec(int(taps.w) + ij.y);
		bool unused;
		ivec4 v1 = bilinear(slot, l0 + 1, tx1, ty1, unused);
		uvec4 V = uvec4(v0) * uint(256 - f) + uvec4(v1) * uint(f);
		return vec4(V / 1044480u) / 4096.0;
	}
	if (exact) {
		ivec4 k = v0 / 65536;
		return vec4(byteFloat(k.r), byteFloat(k.g), byteFloat(k.b), byteFloat(k.a));
	}
	return vec4(v0 / 4080) / 4096.0;
}

// UIGradient multipliers for images (images.rs GradientSampler): block (gx, gy, g0, 0), (colour keys,
// transparency keys, 0, 0) then colour keys (t, r, g, b) and transparency keys (t, v, 0, 0)
vec4 imageGradient(int g, vec2 X) {
	vec4 h = rec(g), counts = rec(g + 1);
	float t = clamp(h.x * X.x + h.y * X.y + h.z, 0.0, 1.0);
	int nc = int(counts.x), nt = int(counts.y);
	vec3 color = vec3(1.0);
	if (nc > 0) {
		vec4 k0 = rec(g + 2);
		if (t <= k0.x) {
			color = k0.yzw;
		} else {
			color = rec(g + 1 + nc).yzw;
			for (int i = 1; i < nc; i++) {
				vec4 a = rec(g + 1 + i), b = rec(g + 2 + i);
				if (t <= b.x) {
					float f = b.x == a.x ? 0.0 : (t - a.x) / (b.x - a.x);
					color = a.yzw + (b.yzw - a.yzw) * f;
					break;
				}
			}
		}
	}
	float fade = 0.0;
	if (nt > 0) {
		int base = g + 2 + nc;
		vec4 k0 = rec(base);
		if (t <= k0.x) {
			fade = k0.y;
		} else {
			fade = rec(base + nt - 1).y;
			for (int i = 1; i < nt; i++) {
				vec4 a = rec(base + i - 1), b = rec(base + i);
				if (t <= b.x) {
					float f = b.x == a.x ? 0.0 : (t - a.x) / (b.x - a.x);
					fade = a.y + (b.y - a.y) * f;
					break;
				}
			}
		}
	}
	return vec4(color, 1.0 - fade);
}

// Shade (tint, ImageTransparency, UIGradient, corner coverage): the straight colour and alpha
vec4 shadeValue(vec4 value, vec2 X, float coverage, int tintAt, int gradientAt) {
	vec4 tint = rec(tintAt);
	vec4 c = vec4(value.rgb * tint.rgb, value.a * tint.a * coverage);
	if (rec(gradientAt).x > 0.5) {
		c *= gImageGradient;
	}
	return c;
}

// Shaded and converted with gpu8. Without a UIGradient the calibrated model multiplies the float32
// sample v by ImageColor3 / 255 and ImageTransparency's alpha8 / 255 in double and rounds once to float32, so
// the byte is floor(v * b * 4096 / 255) of v exactly (gpu8Tinted); a corner coverage below 1 or a gradient
// multiplies in float
ivec4 shadeBytes(vec4 value, vec2 X, float coverage, int tintAt, int gradientAt) {
	if (rec(gradientAt).x > 0.5) {
		return conv12(shadeValue(value, X, coverage, tintAt, gradientAt));
	}
	vec4 tint = rec(tintAt);
	ivec4 b = ivec4(floor(tint * 255.0 + 0.5));
	int a = coverage >= 1.0 ? gpu8Tinted(value.a, b.a) : conv12(value.a * tint.a * coverage);
	return ivec4(gpu8Tinted(value.r, b.r), gpu8Tinted(value.g, b.g), gpu8Tinted(value.b, b.b), a);
}

// Shaded, converted with gpu8: the straight colour and alpha, which the "image" batch blend mixes with the
// target like Studio, round((c A + D (255 - A)) / 255) with one rounding (notes/images.md: a close button)
vec4 shadeImage(vec4 value, vec2 X, float coverage, int tintAt, int gradientAt) {
	ivec4 q = shadeBytes(value, X, coverage, tintAt, gradientAt);
	if (q.a <= 0) {
		DISCARD
	}
	return fromBytes(q);
}

// The UICorner mask of a QW x QH screen quad at the texels of a resampled CanvasGroup texture (images.rs
// corner): texel i's centre (i + off) / s screen px from the quad's corner, derivatives 1 / s (not measured)
float textureCornerMask(vec2 ij, vec4 quad, vec2 s, vec4 radii) {
	vec2 halfSize = quad.xy * 0.5;
	vec2 p = (ij + quad.zw) / s - halfSize;
	float r = quadrantRadius(p, radii);
	vec2 q = abs(p) - halfSize + r;
	float d, fw;
	if (q.x > 0.0 && q.y > 0.0) {
		float len = length(q);
		d = len - r;
		fw = (q.x / s.x + q.y / s.y) / len;
	} else {
		d = max(q.x, q.y) - r;
		fw = q.x >= q.y ? 1.0 / s.x : 1.0 / s.y;
	}
	float t = clamp(0.5 - d / fw, 0.0, 1.0);
	return floor(t * t * (3.0 - 2.0 * t) * 255.0 + 0.5) / 255.0;
}

// IMAGE (snapped quad): quad (X0, Y0, W, H), node rect (L, T, R, B), (slot, l0, f, flags: 1 pixelated,
// 4 corner mask, 8 in a CanvasGroup texture, 16 texture corner mask), tap offsets, tint + fade, corner (mask
// slot, 0, 0, 0) or texture corner radii, gradient (flag, offset), texture corner (QW, QH, offX, offY),
// (sx, sy). In a texture (a resampled CanvasGroup's image, groups.rs group_image) the quad and node rect are
// in texels. The output is the straight colour and alpha
// converted like the GPU; the "image" batch blend mixes it like Studio (on the screen too)
vec4 kindImage(ivec2 P, vec2 X) {
	vec4 nodeRect = rec(4);
	if (X.x < nodeRect.x || X.x >= nodeRect.z || X.y < nodeRect.y || X.y >= nodeRect.w) {
		DISCARD
	}
	bool ok;
	vec4 value = sampleImageQuad(P, ok);
	vec4 quad = rec(3);
	int flags = int(rec(5).w);
	float coverage = 1.0;
	if ((flags & 4) != 0) {
		coverage = imageTexel(int(rec(8).x), P - ivec2(quad.xy), 0).r;
	} else if ((flags & 16) != 0) {
		coverage = textureCornerMask(vec2(P) - quad.xy, rec(10), rec(11).xy, rec(8));
	}
	return shadeImage(value, X, coverage, 7, 9);
}

// IMAGE_ROTATED: inverse, (e, f, ox, oy) to local space, bounds (x0, y0, x1, y1), screen rect,
// (slot, quads, flags: 1 pixelated, 2 quarter turn, 4 rounded, 8 premultiplied texture, 16 hard edges, 32 gradient field, levels), tint + fade, radii, gradient
// (flag, offset), the second triangle's inverse and (e, f, ox, oy), the diagonal (a, b, c: the first triangle
// where a (x - ox) + b (y - oy) + c >= 0; geometry.rs snapped_quad_maps), then from texel 14 per quad
// (x0, y0, x1, y1), (u0, v0, u1, v1), (lod, wrap, unrounded, 0)
// (unrounded: a 9-slice part without texels on an axis, which UICorner leaves square: images.rs ImageQuad::unrounded)
// Wrap / clamp an index into [0, n) (GLSL's % is undefined for negative operands)
int wrapIndex(int i, int n) {
	return i >= 0 ? i % n : n - 1 - ((-i - 1) % n);
}

ivec2 fixIndex(ivec2 i, ivec2 n, bool wrap) {
	return wrap ? ivec2(wrapIndex(i.x, n.x), wrapIndex(i.y, n.y)) : clamp(i, ivec2(0), n - 1);
}

float truncate12(float v) {
	return floor(v * (4096.0 / 255.0)) / 4096.0;
}

vec4 sampleAt(int slot, int levels, vec2 uv, float lod, bool wrap, bool pixelated) {
	if (pixelated) {
		int level = min(levels - 1, max(0, int(floor(max(0.0, lod) + 0.5))));
		ivec2 size = imageSize(slot, level);
		ivec2 i = fixIndex(ivec2(ceil(uv * vec2(size))) - 1, size, wrap);
		return imageTexel(slot, i, level);
	}
	float last = float(levels - 1);
	float clamped = clamp(lod, 0.0, last);
	int l0 = int(floor(clamped));
	int f = int(floor((clamped - float(l0)) * 256.0 + 0.5));
	if (f == 256) {
		l0 += 1;
		f = 0;
	}
	if (l0 >= levels - 1) {
		l0 = levels - 1;
		f = 0;
	}
	vec4 result = vec4(0.0);
	for (int k = 0; k < 2; k++) {
		int level = l0 + k;
		if (k == 1 && f == 0) {
			break;
		}
		ivec2 size = imageSize(slot, level);
		vec2 s = uv * vec2(size) - 0.5;
		ivec2 base = ivec2(floor(s));
		ivec2 w = ivec2(floor((s - floor(s)) * 256.0 + 0.5));
		if (w.x == 256) {
			base.x += 1;
			w.x = 0;
		}
		if (w.y == 256) {
			base.y += 1;
			w.y = 0;
		}
		ivec2 i0 = fixIndex(base, size, wrap), i1 = fixIndex(base + 1, size, wrap);
		vec4 tx = vec4(i0.x, i1.x, w.x, 0.0), ty = vec4(i0.y, i1.y, w.y, 0.0);
		bool unused;
		vec4 v = vec4(bilinear(slot, level, tx, ty, unused)) / 65536.0;
		result = k == 0 ? v : result + (v - result) * (float(f) / 256.0);
	}
	return vec4(truncate12(result.r), truncate12(result.g), truncate12(result.b), truncate12(result.a));
}

float rotatedCoverage(vec2 p, vec2 halfSize, bool rounded, vec4 radii, vec4 m) {
	if (!rounded) {
		// A mitred corner, as a turned frame's (turnedQuad)
		return turnedQuad(p, halfSize);
	}
	if (p.x < -halfSize.x || p.x >= halfSize.x || p.y < -halfSize.y || p.y >= halfSize.y) {
		return 0.0;
	}
	float r = quadrantRadius(p, radii);
	vec2 q = abs(p) - halfSize + r;
	float d;
	vec2 g;
	if (q.x > 0.0 && q.y > 0.0) {
		float len = length(q);
		d = len - r;
		g = sign(p) * q / len;
	} else {
		d = max(q.x, q.y) - r;
		g = q.x >= q.y ? vec2(sign(p.x), 0.0) : vec2(0.0, sign(p.y));
	}
	float fw = abs(g.x * m.x + g.y * m.y) + abs(g.x * m.z + g.y * m.w);
	float t = clamp(0.5 - d / fw, 0.0, 1.0);
	return t * t * (3.0 - 2.0 * t);
}

// Position along an edge of length L at distance d outside it, t from its start corner, as a fraction of L
// (images.rs rotated_fringe_sample, measured)
float fringeAlong(float t, float L, float d) {
	return t < L - d * (L + 1.0) ? (t + d) / L : (t - d + 2.0) / (L + 2.0);
}

vec2 fringeSample(vec2 l, vec4 b) {
	float W = b.z - b.x, H = b.w - b.y;
	float dt = b.y - l.y, dr = l.x - b.z, db = l.y - b.w, dl = b.x - l.x;
	float d = max(max(dt, dr), max(db, dl));
	if (d == dt) {
		return vec2(b.x + fringeAlong(l.x - b.x, W, d) * W, b.y);
	}
	if (d == dr) {
		return vec2(b.z, b.y + fringeAlong(l.y - b.y, H, d) * H);
	}
	if (d == db) {
		return vec2(b.z - fringeAlong(b.z - l.x, W, d) * W, b.w);
	}
	return vec2(b.x, b.w - fringeAlong(b.w - l.y, H, d) * H);
}

// The mip level of a fringe pixel (images.rs rotated_fringe_lod): the strip triangle's texture coordinate does
// not change across the edge and runs along it as fringeAlong lays it, so a pixel spans the edge's texels
// (density: texels a local px along x and y) along one diagonal of the frame; the longest of across, down and
// the two screen diagonals at 11/16 (sampling.rs footprint_length)
float fringeLod(vec2 l, vec4 b, vec2 density, vec4 m) {
	float W = b.z - b.x, H = b.w - b.y;
	float dt = b.y - l.y, dr = l.x - b.z, db = l.y - b.w, dl = b.x - l.x;
	float d = max(max(dt, dr), max(db, dl));
	vec2 g;
	if (d == dt || d == db) {
		float t = d == dt ? l.x - b.x : b.z - l.x;
		bool first = t < W - d * (W + 1.0);
		float s = density.x * (first ? 1.0 : W / (W + 2.0));
		g = vec2(s, (d == dt) == first ? -s : s);
	} else {
		float t = d == dr ? l.y - b.y : b.w - l.y;
		bool first = t < H - d * (H + 1.0);
		float s = density.y * (first ? 1.0 : H / (H + 2.0));
		g = vec2(s, (d == dr) == first ? s : -s);
	}
	float across = g.x * m.x + g.y * m.y, down = g.x * m.z + g.y * m.w;
	return log2(max(max(abs(across), abs(down)), 0.6875 * max(abs(across + down), abs(across - down))));
}

// A pixel centre on the mitre line of a turned quad's corner, where the anti-aliasing strips of two edges meet,
// belongs to one of them by the rasterizer's top-left rule: to the strip of the left or right edge when, from the
// line, that strip lies to the right on the screen, or straight below (bits 16 32 64 128: at TL, TR, BR, BL it
// does). Measured on squares turned 45 degrees about a pixel's centre (fixtures/image-probe-rotlod.luau V_8_8_45,
// V_5_5_45). Returns the local point moved off the line into the strip that owns the pixel.
vec2 ownedLocal(vec2 l, vec4 b, int bits) {
	float dx = max(b.x - l.x, l.x - b.z), dy = max(b.y - l.y, l.y - b.w);
	if (dx > 0.0 && dy > 0.0 && abs(dx - dy) < 0.00025) {
		bool left = b.x - l.x > 0.0, top = b.y - l.y > 0.0;
		int bit = left ? (top ? 16 : 128) : (top ? 32 : 64);
		if ((bits & bit) != 0) {
			l.x += left ? -0.001 : 0.001;
		} else {
			l.y += top ? -0.001 : 0.001;
		}
	}
	return l;
}

vec4 kindImageRotated(ivec2 P, vec2 X) {
	vec4 m = rec(3), tr = rec(4), bounds = rec(5), screen = rec(6), info = rec(7);
	// The quad's corners sit on the rasterizer's 1/256 px grid, so each of its two triangles has its own map
	vec4 diagonal = rec(13);
	if (diagonal.x * (X.x - tr.z) + diagonal.y * (X.y - tr.w) + diagonal.z < 0.0) {
		m = rec(11);
		tr = rec(12);
	}
	vec2 l = toLocal(m, tr, X);
	if (diagonal.w > 0.0) {
		l = ownedLocal(l, bounds, int(diagonal.w));
	}
	vec2 cl = clamp(l, bounds.xy, bounds.zw);
	int slot = int(info.x), count = int(info.y), flags = int(info.z), levels = int(info.w);
	bool pixelated = (flags & 1) != 0, quarter = (flags & 2) != 0, rounded = (flags & 4) != 0;
	int found = -1;
	for (int i = 0; i < count; i++) {
		vec4 qb = rec(14 + i * 3);
		if (cl.x >= qb.x && cl.x <= qb.z && cl.y >= qb.y && cl.y <= qb.w) {
			found = i;
			break;
		}
	}
	if (found < 0) {
		DISCARD
	}
	vec4 qb = rec(14 + found * 3), uv = rec(15 + found * 3), qs = rec(16 + found * 3);
	float coverage;
	if (rounded) {
		vec2 halfSize = (qb.zw - qb.xy) * 0.5;
		coverage = rotatedCoverage(l - qb.xy - halfSize, halfSize, true, qs.z > 0.5 ? vec4(0.0) : rec(9), m);
	} else if (quarter) {
		coverage = X.x >= screen.x && X.x < screen.z && X.y >= screen.y && X.y < screen.w ? 1.0 : 0.0;
	} else if ((flags & 16) != 0) {
		// Flag 16: a CanvasGroup under a UIGradient has no anti-aliased edge (groups.rs composite_turned_group); a
		// pixel centre is in it by the edge functions on its grid corners (texels 6 and 9, as gridQuad reads them)
		coverage = gridCorners(screen, rec(9), P) ? 1.0 : 0.0;
	} else {
		vec2 halfSize = (bounds.zw - bounds.xy) * 0.5;
		coverage = rotatedCoverage(l - bounds.xy - halfSize, halfSize, false, vec4(0.0), m);
	}
	if (coverage <= 0.0) {
		DISCARD
	}
	// A fringe pixel outside the quad samples where Studio's anti-aliasing strip puts it (images.rs
	// rotated_fringe_sample): along the edge shifted by the strip's triangles, across it on the edge line, at
	// the strip's own mip level
	vec2 sp = l;
	bool fringe = !quarter && (l.x < bounds.x || l.x > bounds.z || l.y < bounds.y || l.y > bounds.w);
	if (fringe) {
		sp = fringeSample(l, bounds);
		for (int i = 0; i < count; i++) {
			vec4 qi = rec(14 + i * 3);
			if (sp.x >= qi.x && sp.x <= qi.z && sp.y >= qi.y && sp.y <= qi.w) {
				qb = qi;
				uv = rec(15 + i * 3);
				qs = rec(16 + i * 3);
				break;
			}
		}
	}
	vec2 t = (sp - qb.xy) / (qb.zw - qb.xy);
	vec2 uvp = uv.xy + t * (uv.zw - uv.xy);
	float lod = qs.x;
	if (fringe) {
		lod = fringeLod(l, bounds, vec2(imageSize(slot, 0)) * abs(uv.zw - uv.xy) / (qb.zw - qb.xy), m);
	}
	vec4 value = sampleAt(slot, levels, uvp, lod, qs.y > 0.5, pixelated);
	// Flag 8: the texture holds premultiplied colour (a CanvasGroup's), filtered as it is
	if ((flags & 8) != 0 && value.a > 0.0) {
		value.rgb = min(value.rgb / value.a, vec3(1.0));
	}
	// Flag 32: a CanvasGroup's UIGradient, its field at this pixel's place in the group (image slot 1, over
	// the record's pixels)
	if ((flags & 32) != 0) {
		vec4 f = imageTexel(1, P - ivec2(tr.zw), 0);
		value *= f;
	}
	return shadeImage(value, l, coverage, 8, 10);
}

// TextBox selection clip (selected glyphs white over the selection, the rest as usual): (mode, count, 0, 0) then rects from texel at; mode 1
// draws outside every rect, mode 2 only inside one (pixel-aligned path clips)
bool selectionAllowsAt(int at, vec2 X) {
	vec4 s = rec(at);
	int mode = int(s.x);
	if (mode == 0) {
		return true;
	}
	int count = int(s.y);
	bool inside = false;
	for (int i = 0; i < count; i++) {
		vec4 r = rec(at + 1 + i);
		if (X.x >= r.x && X.x < r.z && X.y >= r.y && X.y < r.w) {
			inside = true;
			break;
		}
	}
	return mode == 1 ? !inside : inside;
}

// Correctly rounded float32 division x / y for x >= 0, y > 0 (normal numbers), as the CPU's
// f32 x / y: long division of the 24-bit mantissas (the GPU's own division is approximate)
float divRN(float x, float y) {
	if (!(x > 0.0)) {
		return 0.0;
	}
	uint bx = floatBitsToUint(x), by = floatBitsToUint(y);
	int e = int(bx >> 23) - int(by >> 23);
	uint mx = (bx & 0x7FFFFFu) | 0x800000u, my = (by & 0x7FFFFFu) | 0x800000u;
	if (mx < my) {
		mx <<= 1;
		e -= 1;
	}
	// mx / my in [1, 2): the leading bit, then 24 more (23 fraction bits and a rounding bit)
	uint r = mx - my, q = 1u;
	for (int i = 0; i < 3; i++) {
		r <<= 8;
		uint digit = r / my;
		r -= digit * my;
		q = (q << 8) | digit;
	}
	uint mantissa = q >> 1;
	if ((q & 1u) != 0u && (r != 0u || (mantissa & 1u) != 0u)) {
		mantissa += 1u;
		if (mantissa == 0x1000000u) {
			mantissa >>= 1;
			e += 1;
		}
	}
	return uintBitsToFloat(uint(e + 127) << 23 | (mantissa & 0x7FFFFFu));
}


// GROUP (groups.rs composite_group: an axis-aligned CanvasGroup's shaded texture as a straight layer, output
// premultiplied as round(c A / 255), premultiplied blend): rect in
// target pixels (x, y, W, H), (flags: 1 tint, 2 corner, 4 gradient, 8 / 16 resampled x / y, 32 straight, opacity
// byte, x taps offset, y taps offset), (tint bytes, gradient block offset), (corner mask slot, 0, 0, 0),
// (0, 0, 0, 0); taps (texel a, texel b, w8, 0) per column / row of the N x N texture (uLayer); gradient
// block as imageGradient's
vec4 kindGroup(ivec2 P, vec2 X) {
	vec4 geo = rec(3), info = rec(4), tint = rec(5);
	int flags = int(info.x);
	ivec2 g = P - ivec2(geo.xy);
	ivec2 a = g, b = g;
	int wx = 0, wy = 0;
	if ((flags & 8) != 0) {
		vec4 tap = rec(int(info.z) + g.x);
		a.x = int(tap.x);
		b.x = int(tap.y);
		wx = int(tap.z);
	}
	if ((flags & 16) != 0) {
		vec4 tap = rec(int(info.w) + g.y);
		a.y = int(tap.x);
		b.y = int(tap.y);
		wy = int(tap.z);
	}
	// Texture sample: a texel centre keeps byte / 255, a filtered one is truncated to 12 bits
	ivec4 t00 = toBytes(texelFetch(sampler2D(uLayer, uPointSampler), a, 0));
	vec4 s;
	if (wx == 0 && wy == 0) {
		s = vec4(byteFloat(t00.r), byteFloat(t00.g), byteFloat(t00.b), byteFloat(t00.a));
	} else {
		ivec4 t10 = toBytes(texelFetch(sampler2D(uLayer, uPointSampler), ivec2(b.x, a.y), 0));
		ivec4 t01 = toBytes(texelFetch(sampler2D(uLayer, uPointSampler), ivec2(a.x, b.y), 0));
		ivec4 t11 = toBytes(texelFetch(sampler2D(uLayer, uPointSampler), b, 0));
		ivec4 V = (t00 * (256 - wx) + t10 * wx) * (256 - wy) + (t01 * (256 - wx) + t11 * wx) * wy;
		s = vec4(V / 4080) / 4096.0;
	}
	if (!(s.a > 0.0)) {
		DISCARD
	}

	// Straight colour and alpha as the calibrated model computes them (float32 steps, conversions like the GPU)
	float alpha = s.a * byteFloat(int(info.y));
	// x / 1 rounds to x itself: opaque texels, the common case, skip the long divisions
	vec3 c = s.a == 1.0 ? s.rgb : vec3(divRN(s.r, s.a), divRN(s.g, s.a), divRN(s.b, s.a));
	ivec3 c8;
	if ((flags & 4) != 0) {
		// The per-vertex field / 255 (float32, from the CPU): alpha = fround(A * fa), colour gpu8 of the exact
		// product of the float32 values
		vec4 f = imageTexel(int(tint.w), P - ivec2(rec(0).xy), 0);
		alpha = s.a * f.a;
		c8 = ivec3(gpu8Mul(c.r, f.r), gpu8Mul(c.g, f.g), gpu8Mul(c.b, f.b));
	} else if ((flags & 1) != 0) {
		c8 = ivec3(gpu8Tinted(c.r, int(tint.r)), gpu8Tinted(c.g, int(tint.g)), gpu8Tinted(c.b, int(tint.b)));
	} else {
		c8 = conv12(vec4(c, 0.0)).rgb;
	}
	if ((flags & 2) != 0) {
		alpha *= imageTexel(int(rec(6).x), g, 0).r;
	}
	int A = conv12(alpha);
	if (A <= 0) {
		DISCARD
	}
	// Flag 32 (a group nested in a resampled group's texture, groups.rs texture_group): the straight
	// colour for the image blend
	if ((flags & 32) != 0) {
		return fromBytes(ivec4(c8, A));
	}
	// The straight bytes as putImageData stores them: round(c A / 255)
	return fromBytes(ivec4(mulDiv255(ivec4(c8, 0), A).rgb, A));
}

// A text quad's image texel (the glyph bitmap with a transparent 1 px border, like the atlas padding),
// premultiplied bytes: mode 3 tints the glyph coverage with the quad's colour, mode 4 reads the
// CPU-shaded image (border included)
ivec4 quadImageTexel(int mode, vec4 h, ivec3 colour, ivec2 ij, ivec2 size) {
	if (mode == 4) {
		return toBytes(imageTexel(int(h.y), ivec2(h.zw) + ij, 0));
	}
	if (ij.x < 1 || ij.y < 1 || ij.x > size.x - 2 || ij.y > size.y - 2) {
		return ivec4(0);
	}
	int g = toByte(texelFetch(sampler2D(uGlyphs, uPointSampler), ivec2(h.zw) + ij - 1, 0).a);
	return ivec4(mulDiv255(ivec4(colour, 0), g).rgb, g);
}

// A glyph quad (or rotated shadow layer) through a rotation or a scale: the quad's image sampled like Chrome's drawImage
// (bilinear, 8-bit weights, clamped to the image, 12-bit truncation, anti-aliased edges), times
// globalAlpha (the vertex alpha for tinted glyphs, 1 for shaded ones). Transform block at rec(2).z:
// target pixel -> image texel, (W, H, 0, 0)
vec4 resampledQuad(int mode, vec4 h, vec2 X) {
	int tr = int(rec(2).z);
	vec4 m = rec(tr), t = rec(tr + 1), size = rec(tr + 2);
	vec2 l = toLocal(m, t, X);
	float du = min(l.x, size.x - l.x) / (abs(m.x) + abs(m.z));
	float dv = min(l.y, size.y - l.y) / (abs(m.y) + abs(m.w));
	float coverage = clamp(min(du, dv) + 0.5, 0.0, 1.0);
	if (coverage <= 0.0) {
		DISCARD
	}
	ivec3 colour = unpackRgb(rec(4).x);
	ivec2 n = ivec2(size.xy);
	vec2 s = l - 0.5;
	ivec2 base = ivec2(floor(s));
	ivec2 w = ivec2(floor((s - floor(s)) * 256.0 + 0.5));
	if (w.x == 256) {
		base.x += 1;
		w.x = 0;
	}
	if (w.y == 256) {
		base.y += 1;
		w.y = 0;
	}
	ivec2 last = n - 1;
	ivec2 i0 = clamp(base, ivec2(0), last), i1 = clamp(base + 1, ivec2(0), last);
	ivec4 a = quadImageTexel(mode, h, colour, i0, n);
	vec4 value;
	if (w.x == 0 && w.y == 0) {
		value = vec4(a) / 255.0;
	} else {
		ivec4 b = quadImageTexel(mode, h, colour, ivec2(i1.x, i0.y), n);
		ivec4 c = quadImageTexel(mode, h, colour, ivec2(i0.x, i1.y), n);
		ivec4 d = quadImageTexel(mode, h, colour, i1, n);
		ivec4 V = (a * (256 - w.x) + b * w.x) * (256 - w.y) + (c * (256 - w.x) + d * w.x) * w.y;
		value = vec4(V / 4080) / 4096.0;
	}
	if (value.a <= 0.0) {
		DISCARD
	}
	float G = (mode == 3 ? h.y : rec(4).y) / 255.0;
	return value * G * coverage;
}

// TEXT_QUAD (scene_draw.rs text_quad, one glyph quad of rotated text): (mode, a, x, y), (rgb packed),
// selection block. Modes (resampledQuad): 3 the glyph (texture x, y) tinted with the colour at globalAlpha
// a / 255, 4 a CPU image (atlas slot, atlas x, y: gradient shading or a tinted decoration rect) at
// globalAlpha rec(4).y / 255.
vec4 kindTextQuad(ivec2 P, vec2 X) {
	if (!selectionAllowsAt(5, X)) {
		DISCARD
	}
	vec4 h = rec(3);
	return resampledQuad(int(h.x + 0.5), h, X);
}

// SEGMENT (groups.rs flush_segment): texel (k, l) of a CanvasGroup texture takes the pixel of the
// screen scratch (uLayer) under its centre, (floor((2k + 1) W / (2 NW)), floor((2l + 1) H / (2 NH))), the centre on the 1/256 texel grid, as
// premultiplied bytes; the batch's premultiplied blend is round(C + D (255 - A) / 255). (W, H, NW, NH),
// (smooth, 0, 0, 0): smooth samples the scratch bilinearly at the texel centre instead (text; exact rational
// position ((2k + 1) W - N) / (2N), 8-bit weights, rounded half up; notes/images.md)
ivec2 segmentFloor(ivec2 num, ivec2 den) {
	return ivec2(num.x >= 0 ? num.x / den.x : -((-num.x + den.x - 1) / den.x), num.y >= 0 ? num.y / den.y : -((-num.y + den.y - 1) / den.y));
}
vec4 kindSegment(ivec2 P, vec2 X) {
	vec4 h = rec(3);
	ivec2 W = ivec2(h.xy), N = ivec2(h.zw);
	if (rec(4).x > 0.5) {
		ivec2 num = (2 * P + 1) * W - N, den = 2 * N;
		ivec2 f = segmentFloor(num, den);
		ivec2 w = ((num - f * den) * 256 + N) / den;
		if (w.x == 256) { f.x += 1; w.x = 0; }
		if (w.y == 256) { f.y += 1; w.y = 0; }
		ivec2 a = clamp(f, ivec2(0), W - 1), b = clamp(f + 1, ivec2(0), W - 1);
		ivec4 t00 = toBytes(texelFetch(sampler2D(uLayer, uPointSampler), a, 0));
		ivec4 t10 = toBytes(texelFetch(sampler2D(uLayer, uPointSampler), ivec2(b.x, a.y), 0));
		ivec4 t01 = toBytes(texelFetch(sampler2D(uLayer, uPointSampler), ivec2(a.x, b.y), 0));
		ivec4 t11 = toBytes(texelFetch(sampler2D(uLayer, uPointSampler), b, 0));
		ivec4 V = (t00 * (256 - w.x) + t10 * w.x) * (256 - w.y) + (t01 * (256 - w.x) + t11 * w.x) * w.y;
		ivec4 v = (V + 32768) / 65536;
		if (v.a == 0) {
			DISCARD
		}
		return fromBytes(v);
	}
	// What is drawn into the texture has its edges on the rasterizer's 1/256 texel grid (rounded to the
	// nearest): a texel whose centre lies within 1/512 texel left of or above a pixel's edge takes that
	// pixel (fixtures/group-probe-bigtex.luau Row8.Bar8: the edge 0.0014 texel right of a centre)
	ivec2 num = (2 * P + 1) * W, den = 2 * N;
	ivec2 at = num / den, rest = num - at * den;
	// (component by component: WebGL2's GLSL has no mix() of integer vectors, which a vector cast becomes there)
	ivec2 edge = 1024 * N - 2 * W;
	at += ivec2(rest.x * 512 >= edge.x ? 1 : 0, rest.y * 512 >= edge.y ? 1 : 0);
	ivec4 texel = toBytes(texelFetch(sampler2D(uLayer, uPointSampler), min(at, W - 1), 0));
	if (texel.a == 0) {
		DISCARD
	}
	return fromBytes(texel);
}

// LAYER (a layer computed on the CPU and drawn unscaled: frame and image gradients, rounded fills, text):
// (0, slot, atlas x, atlas y), selection block; its premultiplied bytes in the slot's texture at texel
// P - quad corner + (atlas x, atlas y) (gpu.rs LayerAtlas; zero for a texture of its own)
vec4 kindLayer(ivec2 P, vec2 X) {
	if (!selectionAllowsAt(4, X)) {
		DISCARD
	}
	vec4 h = rec(3);
	ivec4 v = toBytes(imageTexel(int(h.y), P - ivec2(rec(0).xy) + ivec2(h.zw), 0));
	if (v.a == 0) {
		DISCARD
	}
	return fromBytes(v);
}

// GRADIENT_FILL (gradient atlas pass): a gradient layer's quad from the target origin, paint at rec(3)
vec4 kindGradientFill(ivec2 P, vec2 X) {
	return gPaint;
}

void main() {
	vec2 X = gl_FragCoord.xy;
	ivec2 P = ivec2(X);
	vec4 clip = rec(1);
	if (X.x < clip.x || X.x >= clip.z || X.y < clip.y || X.y >= clip.w) {
		discard;
	}
	int kind = int(rec(2).x);
	// Paints and image gradients of the kinds that use them, once
	int paintAt = 0;
	if (kind == 1) {
		paintAt = 4;
	} else if (kind == 2) {
		paintAt = 5;
	} else if (kind == 3) {
		paintAt = 9;
	} else if (kind == 5 && rec(13).x > 0.5) {
		paintAt = 14;
	} else if (kind == 13) {
		paintAt = 3;
	}
	if (paintAt > 0) {
		gPaint = paintFloat(paintAt, X);
	}
	int gradientAt = -1;
	vec2 gradientX = X;
	if (kind == 6 && rec(9).x > 0.5) {
		gradientAt = int(rec(9).y);
	} else if (kind == 7 && rec(10).x > 0.5) {
		gradientAt = int(rec(10).y);
		gradientX = toLocal(rec(3), rec(4), X);
	}
	if (gradientAt >= 0) {
		gImageGradient = imageGradient(gradientAt, gradientX);
	}
	switch (kind) {
		case 1: outColor = kindRect(P, X); break;
		case 2: outColor = kindRounded(P, X); break;
		case 3: outColor = kindRotated(P, X); break;
		case 4: outColor = kindShadow(P, X); break;
		case 5: outColor = kindStroke(P, X); break;
		case 6: outColor = kindImage(P, X); break;
		case 7: outColor = kindImageRotated(P, X); break;
		case 11: outColor = kindGroup(P, X); break;
		case 15: outColor = kindTextQuad(P, X); break;
		case 17: outColor = kindSegment(P, X); break;
		case 18: outColor = kindLayer(P, X); break;
		case 13: outColor = kindGradientFill(P, X); break;
		default: gDiscard = true; break;
	}
	if (gDiscard) {
		discard;
	}
}
