// The uber-shader's vertex stage. This file (with uber.frag.glsl) is the hand-maintained source, not generated.
// Each instance covers its record's pixel bounds. Gradient coordinates are interpolated like Skia's: Ganesh
// computes the gradient matrix times the quad's local coordinates in the vertex shader and the rasterizer
// interpolates it, so a record whose quad is the gradient's own quad (rec(2).w = its gradient block) does the
// same on the same vertices, corners TL, BL, TR, BR as a strip like GrQuad (the corners buffer in gpu.rs). Keep the
// calibrated arithmetic as it is.
#version 450
layout(set=1,binding=12) uniform sampler uPointSampler;

layout(location = 0) in uint aRecord;
// Quad corner (0/1, 0/1): instanced draws on ANGLE/D3D need a per-vertex (divisor 0) attribute
layout(location = 1) in vec2 aCorner;
layout(set=1,binding=0) uniform texture2D uData;
// Skia's sk_RTAdjust for a top-left origin target: (2 / w, -1, 2 / h, -1)
layout(set=0,binding=0) uniform Target { vec4 uAdjust; };
layout(location=0) flat out int vRecord;
layout(location=1) out vec2 vGradient;
// The texture coordinate of an image quad drawn on its own vertices (rec(2).y = the record's coordinate
// texel): Studio's rasterizer interpolates it over the quad's two triangles and its sampler picks the texel,
// so a pixel centre on a texel boundary falls as the hardware rounds it. The same vertices, corner order and
// coordinates give the same texels.
layout(location=2) out vec2 vUv;

vec4 rec(int i) {
	return texelFetch(sampler2D(uData, uPointSampler), ivec2(i & 2047, i >> 11), 0);
}

void main() {
	int r = int(aRecord);
	vec4 b = rec(r);
	float g = rec(r + 2).w;
	// Only an image record keeps a coordinate offset there (a rectangle keeps its hole flag)
	vec4 head = rec(r + 2);
	float uvAt = int(head.x) == 6 ? head.y : 0.0;
	vUv = vec2(0.0);
	vec2 corner = aCorner;
	if (uvAt > 0.0) {
		vec4 uv = rec(r + int(uvAt));
		b = rec(r + int(uvAt) + 1);
		if (fract(uvAt) > 0.25) {
			corner = vec2(1.0 - aCorner.y, aCorner.x);
		}
		vUv = vec2(corner.x > 0.5 ? uv.z : uv.x, corner.y > 0.5 ? uv.w : uv.y);
	} else if (!(g > 0.0)) {
		// Skip rasterizing what the clip test would discard: the quad shrinks to the clip grown by a
		// pixel (vertex snapping cannot lose a pixel centre the fragment clip test keeps) and never
		// grows. Gradient quads keep their vertices: their coordinate is interpolated from them.
		vec4 c = rec(r + 1);
		b = vec4(max(b.xy, floor(c.xy) - 1.0), min(b.zw, ceil(c.zw) + 1.0));
		if (b.x >= b.z || b.y >= b.w) {
			b = vec4(0.0);
		}
	}
	vec2 p = vec2(corner.x > 0.5 ? b.z : b.x, corner.y > 0.5 ? b.w : b.y);
	// wgpu framebuffer and texture rows are top down: the top row maps to clip y = +1 (fragment positions stay
	// top down)
	gl_Position = vec4(p.x * uAdjust.x + uAdjust.y, -(p.y * uAdjust.z + uAdjust.w), 0.0, 1.0);
	vRecord = r;
	vGradient = vec2(0.0);
	if (g > 0.0) {
		int k = r + int(g);
		vec4 m = rec(k + 1), tr = rec(k + 2), l0 = rec(k + 3), l1 = rec(k + 4);
		vec2 local = vec2(l0.x * p.x + l0.z * p.y + l1.x, l0.y * p.x + l0.w * p.y + l1.y);
		vGradient = mat3x2(m.x, m.y, m.z, m.w, tr.x, tr.y) * vec3(local, 1.0);
	}
}
