/* In-process glyph rasterizer: a C port of tools/glyphs/rasterize.py (and metrics.py's hinted
 * truncation advances) on the FreeType 2.14.2 build.rs compiles. Same size request, hinting mode,
 * synthetic bold / italic and stroker settings as the Python tools, so the coverage bitmaps are
 * byte-identical to theirs. Shared with the web build (rur-wasm's native/web/rur_glyph.c is the origin).
 *
 * FreeType objects are not thread-safe: every RurContext owns its FT_Library, and its faces are used
 * by one thread at a time (src/glyphs.rs keeps a context and its faces in one Rasterizer).
 *
 *   rur_glyph_context_new() / rur_glyph_context_free(context)   (free the faces first)
 *   rur_glyph_face_open(context, bytes, length) -> face or NULL  (copies the font bytes)
 *   rur_glyph_face_close(face)
 *   rur_glyph_render(face, ppem, glyph, stroke_px, join, bold, italic, &out) -> 0 on success; out holds a
 *       malloc'd width * rows coverage buffer (tightly packed), left and top; rur_glyph_free(buffer)
 *   rur_glyph_layers(face, glyph, maximum, glyphs, rgba, foreground) -> the glyph's COLR layer count (0: not
 *       a colour glyph), the first `maximum` written: layer glyph, straight RGBA from palette 0, and whether
 *       the layer takes the text colour (palette index 0xFFFF)
 *   rur_glyph_truncation_advances(face, maximum, &glyph, advances) -> 0 on success
 *   rur_glyph_version(context, &major, &minor, &patch)
 */
#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_GLYPH_H
#include FT_STROKER_H
#include FT_OUTLINE_H
#include FT_MODULE_H
#include FT_DRIVER_H
#include FT_TRUETYPE_TABLES_H
#include FT_TRUETYPE_TAGS_H
#include FT_COLOR_H
#include <math.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    int width, rows, left, top;
    unsigned char *buffer;
} RurGlyph;

typedef struct {
    FT_Library library;
} RurContext;

typedef struct {
    RurContext *context;
    FT_Face face;
    unsigned char *bytes;
    FT_Int32 flags;
    long height_units;
    /* ascender - descender, the dimension FreeType's size request scales (height_units less the line gap) */
    long real_units;
    long upm;
} RurFace;

RurContext *rur_glyph_context_new(void) {
    RurContext *context = calloc(1, sizeof(RurContext));
    if (!context) return NULL;
    if (FT_Init_FreeType(&context->library) != 0) {
        free(context);
        return NULL;
    }
    return context;
}

void rur_glyph_context_free(RurContext *context) {
    if (!context) return;
    FT_Done_FreeType(context->library);
    free(context);
}

/* The FreeType version compiled in (the atlas JSON's "freetype" field) */
void rur_glyph_version(RurContext *context, int *major, int *minor, int *patch) {
    FT_Library_Version(context->library, major, minor, patch);
}

/* rasterize.py unhinted_truetype: TrueType outlines without hinting bytecode (maxSizeOfInstructions == 0
 * or no fpgm) are auto-hinted */
static int unhinted_truetype(FT_Face face) {
    FT_ULong length = 0;
    if (FT_Load_Sfnt_Table(face, TTAG_glyf, 0, NULL, &length) != 0) return 0;
    TT_MaxProfile *maxp = (TT_MaxProfile *)FT_Get_Sfnt_Table(face, FT_SFNT_MAXP);
    int max_instructions = maxp ? maxp->maxSizeOfInstructions : 0;
    length = 0;
    int has_fpgm = FT_Load_Sfnt_Table(face, TTAG_fpgm, 0, NULL, &length) == 0;
    return max_instructions == 0 || !has_fpgm;
}

/* rasterize.py unmapped_glyphs_to_fallback_style: glyphs the cmap does not reach get the auto-hinter's
 * fallback style (the style of .notdef), like a FreeType build without HarfBuzz coverage */
static void unmapped_glyphs_to_fallback_style(FT_Library library, FT_Face face) {
    FT_Prop_GlyphToScriptMap prop;
    prop.face = face;
    prop.map = NULL;
    if (FT_Property_Get(library, "autofitter", "glyph-to-script-map", &prop) != 0 || !prop.map) return;
    unsigned char *mapped = calloc((size_t)face->num_glyphs + 1, 1);
    if (!mapped) return;
    FT_UInt index;
    FT_ULong code = FT_Get_First_Char(face, &index);
    while (index != 0) {
        if (index < (FT_UInt)face->num_glyphs) mapped[index] = 1;
        code = FT_Get_Next_Char(face, code, &index);
    }
    FT_UShort fallback = prop.map[0];
    for (FT_Long glyph = 0; glyph < face->num_glyphs; glyph++) {
        if (!mapped[glyph]) prop.map[glyph] = fallback;
    }
    free(mapped);
}

RurFace *rur_glyph_face_open(RurContext *context, const unsigned char *bytes, size_t length) {
    if (!context || !bytes || length == 0) return NULL;
    RurFace *face = calloc(1, sizeof(RurFace));
    if (!face) return NULL;
    face->bytes = malloc(length);
    if (!face->bytes) {
        free(face);
        return NULL;
    }
    memcpy(face->bytes, bytes, length);
    if (FT_New_Memory_Face(context->library, face->bytes, (FT_Long)length, 0, &face->face) != 0) {
        free(face->bytes);
        free(face);
        return NULL;
    }
    FT_Face ft = face->face;
    face->context = context;
    face->flags = unhinted_truetype(ft) ? FT_LOAD_FORCE_AUTOHINT : FT_LOAD_DEFAULT;
    /* the line height requested as S + 1 px includes the hhea line gap (layout.rs height_units) */
    TT_HoriHeader *hhea = (TT_HoriHeader *)FT_Get_Sfnt_Table(ft, FT_SFNT_HHEA);
    face->real_units = ft->ascender - ft->descender;
    face->height_units = face->real_units + (hhea ? hhea->Line_Gap : 0);
    face->upm = ft->units_per_EM;
    if (face->flags & FT_LOAD_FORCE_AUTOHINT) unmapped_glyphs_to_fallback_style(context->library, ft);
    /* FreeType 2.14 separates dotted Latin letters natively: rasterize.py's i/j backport is off */
    return face;
}

void rur_glyph_face_close(RurFace *face) {
    if (!face) return;
    FT_Done_Face(face->face);
    free(face->bytes);
    free(face);
}

/* Python floor division */
static long long floor_div(long long a, long long b) {
    return (a >= 0) == (b > 0) ? a / b : -((llabs(a) + llabs(b) - 1) / llabs(b));
}

/* rasterize.py set_ppem: the line height S + 1 requested as exact 16.16 scales. Python's round() is
 * round-half-even, like nearbyint in the default rounding mode. A face with a line gap gets its
 * ascender - descender requested at its share of the line, cut to 26.6 (layout.rs scales). */
static int set_ppem(RurFace *face, double ppem) {
    long long hu = face->height_units;
    long long real = face->real_units;
    if (hu == 0 || real == 0 || face->upm == 0) return -1;
    long long line26 = (long long)nearbyint(ppem * (double)face->height_units / (double)face->upm * 64.0);
    long long real26 = hu == real ? line26 : floor_div(line26 * real, hu);
    long long scale = floor_div(real26 * 65536 + real / 2, real);
    FT_Size_RequestRec request;
    request.type = FT_SIZE_REQUEST_TYPE_SCALES;
    request.width = (FT_Long)scale;
    request.height = (FT_Long)scale;
    request.horiResolution = 0;
    request.vertResolution = 0;
    return FT_Request_Size(face->face, &request);
}

static const FT_Matrix OBLIQUE = {0x10000, 0x0366A, 0, 0x10000};

static int copy_bitmap(const FT_Bitmap *bitmap, int left, int top, RurGlyph *out) {
    out->left = left;
    out->top = top;
    out->width = 0;
    out->rows = 0;
    out->buffer = NULL;
    if (bitmap->width == 0 || bitmap->rows == 0) return 0;
    out->buffer = malloc((size_t)bitmap->width * bitmap->rows);
    if (!out->buffer) return -3;
    out->width = (int)bitmap->width;
    out->rows = (int)bitmap->rows;
    for (int row = 0; row < out->rows; row++) {
        const unsigned char *source = bitmap->pitch >= 0
            ? bitmap->buffer + (size_t)row * bitmap->pitch
            : bitmap->buffer + (size_t)(out->rows - 1 - row) * (-bitmap->pitch);
        memcpy(out->buffer + (size_t)row * out->width, source, (size_t)out->width);
    }
    return 0;
}

/* join: 0 round, 1 miter (MITER_VARIABLE, limit 2.0), 2 bevel. bold: strength in 26.6 units (0: none).
 * light: 0 the face's own hinting, 1 light hinting (quarter-pixel text), 2 unhinted.
 * A glyph FreeType cannot load or stroke is empty (rasterize.py's FT_Exception), not an error. */
int rur_glyph_render(RurFace *face, double ppem, unsigned glyph, double stroke_px, int join, long bold, int italic,
                     int light, long shift, RurGlyph *out) {
    memset(out, 0, sizeof(*out));
    if (!face) return -1;
    if (set_ppem(face, ppem) != 0) return -2;
    FT_Face ft = face->face;
    /* light 2: the outline as designed, no hinting (the glyphs of a text shadow's glow) */
    FT_Int32 flags = light == 2 ? (FT_LOAD_NO_HINTING | FT_LOAD_NO_BITMAP)
                     : light ? (face->flags | FT_LOAD_TARGET_LIGHT | FT_LOAD_NO_BITMAP) : face->flags;
    if (stroke_px != 0) {
        if (FT_Load_Glyph(ft, glyph, flags | FT_LOAD_NO_BITMAP) != 0) return 0;
        if (shift && ft->glyph->format == FT_GLYPH_FORMAT_OUTLINE) FT_Outline_Translate(&ft->glyph->outline, shift, 0);
        if (bold) FT_Outline_EmboldenXY(&ft->glyph->outline, bold, bold);
        if (italic) FT_Outline_Transform(&ft->glyph->outline, &OBLIQUE);
        FT_Glyph copy;
        if (FT_Get_Glyph(ft->glyph, &copy) != 0) return 0;
        FT_Stroker stroker;
        if (FT_Stroker_New(face->context->library, &stroker) != 0) {
            FT_Done_Glyph(copy);
            return -3;
        }
        FT_Stroker_LineJoin line_join = join == 1 ? FT_STROKER_LINEJOIN_MITER_VARIABLE
                                        : join == 2 ? FT_STROKER_LINEJOIN_BEVEL
                                                    : FT_STROKER_LINEJOIN_ROUND;
        FT_Fixed miter = join == 1 ? (2 << 16) : 0;
        FT_Stroker_Set(stroker, (FT_Fixed)nearbyint(stroke_px * 64.0), FT_STROKER_LINECAP_ROUND, line_join, miter);
        int error = FT_Glyph_Stroke(&copy, stroker, 1);
        FT_Stroker_Done(stroker);
        if (error == 0) error = FT_Glyph_To_Bitmap(&copy, FT_RENDER_MODE_NORMAL, NULL, 1);
        int result = 0;
        if (error == 0) {
            FT_BitmapGlyph bitmap = (FT_BitmapGlyph)copy;
            result = copy_bitmap(&bitmap->bitmap, bitmap->left, bitmap->top, out);
        }
        FT_Done_Glyph(copy);
        return result;
    }
    if (FT_Load_Glyph(ft, glyph, flags | ((bold || italic) ? FT_LOAD_NO_BITMAP : 0)) != 0) return 0;
    if (shift && ft->glyph->format == FT_GLYPH_FORMAT_OUTLINE) FT_Outline_Translate(&ft->glyph->outline, shift, 0);
    if (bold) FT_Outline_EmboldenXY(&ft->glyph->outline, bold, bold);
    if (italic) FT_Outline_Transform(&ft->glyph->outline, &OBLIQUE);
    if (FT_Render_Glyph(ft->glyph, FT_RENDER_MODE_NORMAL) != 0) return 0;
    return copy_bitmap(&ft->glyph->bitmap, ft->glyph->bitmap_left, ft->glyph->bitmap_top, out);
}

void rur_glyph_free(unsigned char *buffer) { free(buffer); }

/* A colour glyph's COLR (v0) layers, bottom first, as FreeType's FT_Render_Glyph walks them with
 * FT_LOAD_COLOR; the colours from palette 0 (FT_Palette_Select). The caller rasterizes each layer glyph
 * like any other glyph: Studio draws the layers as separate glyph quads, each blended on its own
 * (fixtures/text-probe-emoji.luau "Half": an eye over the face at TextTransparency 0.5 is the face's half
 * blend blended again, not half of the opaque emoji). */
int rur_glyph_layers(RurFace *face, unsigned glyph, int maximum, unsigned *glyphs, unsigned char *rgba, int *foreground) {
    if (!face) return 0;
    FT_Face ft = face->face;
    if (!FT_HAS_COLOR(ft)) return 0;
    FT_Color *palette = NULL;
    FT_Palette_Data data;
    if (FT_Palette_Data_Get(ft, &data) != 0 || data.num_palettes == 0 || FT_Palette_Select(ft, 0, &palette) != 0) palette = NULL;
    FT_LayerIterator iterator;
    iterator.p = NULL;
    FT_UInt layer_glyph, color_index;
    int count = 0;
    while (FT_Get_Color_Glyph_Layer(ft, glyph, &layer_glyph, &color_index, &iterator)) {
        if (count < maximum) {
            glyphs[count] = layer_glyph;
            int text = color_index == 0xFFFF || !palette || color_index >= data.num_palette_entries;
            foreground[count] = text;
            unsigned char *out = rgba + (size_t)count * 4;
            if (text) {
                out[0] = out[1] = out[2] = out[3] = 255;
            } else {
                FT_Color color = palette[color_index];
                out[0] = color.red;
                out[1] = color.green;
                out[2] = color.blue;
                out[3] = color.alpha;
            }
        }
        count++;
    }
    return count;
}

/* metrics.py generate: the hinted advance (26.6 / 64) of the truncation glyph (U+2026, else ".") at the
 * physical sizes 1..maximum, the size requested like the renderer's (S + 1) * upm / (ascender - descender + lineGap).
 * advances holds maximum doubles. */
int rur_glyph_truncation_advances(RurFace *face, int maximum, unsigned *glyph_out, double *advances) {
    if (!face || maximum < 1) return -1;
    FT_Face ft = face->face;
    FT_UInt glyph = FT_Get_Char_Index(ft, 0x2026);
    if (!glyph) glyph = FT_Get_Char_Index(ft, '.');
    *glyph_out = glyph;
    if (face->height_units == 0) return -2;
    for (int size = 1; size <= maximum; size++) {
        /* Python: (size + 1) * upm / height_units, an exact integer product, correctly rounded division */
        double ppem = (double)((long long)(size + 1) * face->upm) / (double)face->height_units;
        if (set_ppem(face, ppem) != 0) return -2;
        if (FT_Load_Glyph(ft, glyph, face->flags) != 0) return -3;
        advances[size - 1] = (double)ft->glyph->advance.x / 64.0;
    }
    return 0;
}
