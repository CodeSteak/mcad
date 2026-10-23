/* controller-cad facade over SMLib / SM_API: a flat C ABI with integer handles.
 *
 * Conventions
 *  - Every shape is an integer handle (uint32_t, 0 = none). Handles are immutable:
 *    operations never modify their inputs, they return new handles.
 *  - Functions that create a shape return its handle, or 0 on failure.
 *  - Functions that return int32_t return an SC_* status (or a boolean 0/1, or -1 on error,
 *    where documented).
 *  - After every call, sc_last_status() / sc_last_message() describe the outcome.
 *    sc_last_edges() lists input edge ids that caused a failure, if known.
 *  - Bulk results (queries, meshes) are written to per-call output buffers, read with
 *    sc_out_f64/f32/u32(slot) + sc_out_*_len(slot). They stay valid until the next call.
 *  - No C++ exception escapes: every entry point catches everything.
 *  - Not thread safe (SMLib's default context is process-global). One instance per worker.
 *
 * Provenance: every face carries a tag (u32) as an SMLib attribute (SmLongAttribute,
 * id SC_TAG_ATTR_ID). See sc_facade.cpp, "Provenance".
 */
#ifndef SC_FACADE_H
#define SC_FACADE_H

#include <stdint.h>

#if defined(__EMSCRIPTEN__)
#include <emscripten/emscripten.h>
#define SC_API EMSCRIPTEN_KEEPALIVE
#else
#define SC_API __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* status codes */
#define SC_OK 0
#define SC_FAILED 1
#define SC_UNSUPPORTED 2
#define SC_INVALID 3

/* boolean ops */
#define SC_UNION 0
#define SC_DIFFERENCE 1
#define SC_INTERSECTION 2

/* edge flags (sc_edges) */
#define SC_EDGE_SMOOTH 1 /* nothing to blend: seam, pole, tangent-continuous or open edge */

/* classification */
#define SC_INSIDE 0
#define SC_OUTSIDE 1
#define SC_ON 2

/* Profile loop encoding (f64 array), used by extrude/revolve/loft/sweep:
 *   kind, count, payload..., frame[16]
 *   kind 0 = polygon:  count = number of points, payload = x0 y0 x1 y1 ...
 *   kind 1 = circle:   count = 3, payload = cx cy r
 *   kind 2 = spline:   count = number of points, payload = x0 y0 ... (closed interpolating)
 *   frame = 4x4 column-major matrix (like glam::DMat4::to_cols_array) mapping the
 *           profile's xy-plane to model space.
 * Several loops are simply concatenated. */
#define SC_LOOP_POLYGON 0
#define SC_LOOP_CIRCLE 1
#define SC_LOOP_SPLINE 2

SC_API void sc_init(void);
SC_API uint32_t sc_version(void);
/* Draft mode (0/1): results only serve a temporary display; booleans skip the
 * plausibility check while SMLib succeeds, fillets the self-overlap check. */
SC_API void sc_set_draft(int32_t on);

/* errors */
SC_API int32_t sc_last_status(void);
SC_API const char* sc_last_message(void);
SC_API uint32_t sc_last_message_len(void);
SC_API const uint32_t* sc_last_edges(void);
SC_API uint32_t sc_last_edges_len(void);

/* output buffers (slot 0..7) */
SC_API const double* sc_out_f64(uint32_t slot);
SC_API uint32_t sc_out_f64_len(uint32_t slot);
SC_API const float* sc_out_f32(uint32_t slot);
SC_API uint32_t sc_out_f32_len(uint32_t slot);
SC_API const uint32_t* sc_out_u32(uint32_t slot);
SC_API uint32_t sc_out_u32_len(uint32_t slot);

/* handle table */
SC_API void sc_retain(uint32_t h);
SC_API void sc_release(uint32_t h);
SC_API uint32_t sc_live_handles(void);

/* primitives */
SC_API uint32_t sc_make_box(double x0, double y0, double z0, double x1, double y1, double z1,
                            uint32_t tag);
SC_API uint32_t sc_make_sphere(double r, uint32_t tag);
SC_API uint32_t sc_make_cylinder(double h, double r1, double r2, uint32_t tag);
SC_API uint32_t sc_make_torus(double big_r, double r, uint32_t tag);
/* points: xyz per point; face_sizes[i] = vertex count of face i; indices: concatenated. */
SC_API uint32_t sc_make_polyhedron(const double* points, uint32_t n_points,
                                   const uint32_t* face_sizes, uint32_t n_faces,
                                   const uint32_t* indices, uint32_t n_indices, uint32_t tag);

/* operations */
SC_API uint32_t sc_transform(uint32_t h, const double* m16);
SC_API uint32_t sc_boolean(int32_t op, const uint32_t* hs, uint32_t n, uint32_t tag);
SC_API uint32_t sc_fillet(uint32_t h, const uint32_t* edges, uint32_t n, double r, uint32_t tag);
SC_API uint32_t sc_chamfer(uint32_t h, const uint32_t* edges, uint32_t n, double d, uint32_t tag);
/* Variable-radius fillet (since facade version 4). The radius along edge edges[i] is a law of
 * counts[i] values: a clamped B-spline of degree min(3, m - 1) over the edge's normalised
 * parameter t (0 at the start of its interval, 1 at the end) with uniform interior knots, its
 * control radius j at the Greville abscissa t_j. sc_law_stations writes the edge points at the
 * t_j: f64[0] = xyz per station (edges in order), f64[1] = t_j. sc_fillet_var takes the radii
 * at those stations, concatenated (sum of counts values, all > 0); an edge whose values are
 * all equal is rounded at that constant radius. */
SC_API int32_t sc_law_stations(uint32_t h, const uint32_t* edges, uint32_t n, const uint32_t* counts);
SC_API uint32_t sc_fillet_var(uint32_t h, const uint32_t* edges, uint32_t n, const uint32_t* counts,
                              const double* radii, uint32_t tag);
SC_API uint32_t sc_extrude(const double* loop, uint32_t len, double height, double twist,
                           double scale_x, double scale_y, uint32_t tag);
SC_API uint32_t sc_revolve(const double* loop, uint32_t len, double angle, uint32_t tag);
SC_API uint32_t sc_loft(const double* loops, uint32_t len, uint32_t n_sections, int32_t ruled,
                        uint32_t tag);
/* loop frame is ignored; path: xyz per point; interp 0 = linear, 1 = spline. */
SC_API uint32_t sc_sweep(const double* loop, uint32_t len, const double* path, uint32_t n_path,
                         int32_t interp, uint32_t tag);
/* rows x cols points, row-major, xyz each; interpolating B-spline of degree min(3, n - 1) in
 * each direction (n = 2: bilinear, 3: quadratic) */
SC_API uint32_t sc_nurbs_surface(const double* pts, uint32_t rows, uint32_t cols, uint32_t tag);
SC_API uint32_t sc_thicken(uint32_t h, double t, uint32_t tag);
/* Inward shell. SMLib's shell first; if it fails or its result overlaps itself, the shell is
 * built from Booleans (body minus its inward offset minus prisms over the offsets of the
 * planar opened faces; see sc_facade.cpp, "shell from booleans"). */
SC_API uint32_t sc_shell(uint32_t h, double t, const uint32_t* open_faces, uint32_t n,
                         uint32_t tag);

/* queries */
/* edges: f64[0] = points (xyz), u32[0] = per edge: n_points, n_tags, flags (SC_EDGE_*), tags...
 * (flags since facade version 4) */
SC_API int32_t sc_edges(uint32_t h, uint32_t samples);
/* faces: f64[0] = points (xyz), u32[0] = per face: tag, n_points */
SC_API int32_t sc_faces(uint32_t h, uint32_t samples);
/* classify: u32[0] = SC_INSIDE/SC_OUTSIDE/SC_ON per point */
SC_API int32_t sc_classify(uint32_t h, const double* pts, uint32_t n);
/* tessellate: f32[0] positions, f32[1] normals, f32[2] edge segments (xyz xyz);
 *             u32[0] indices, u32[1] tri tags, u32[2] tri faces, u32[3] edge ids */
SC_API int32_t sc_tessellate(uint32_t h, double tol);
/* mass props: f64[0] = volume, area, cx, cy, cz */
SC_API int32_t sc_mass_props(uint32_t h);
/* bounds: f64[0] = min xyz, max xyz (empty for an empty shape) */
SC_API int32_t sc_bounds(uint32_t h);
SC_API int32_t sc_is_empty(uint32_t h);
SC_API int32_t sc_is_solid(uint32_t h);
SC_API int32_t sc_face_count(uint32_t h);
SC_API int32_t sc_edge_count(uint32_t h);

/* B-rep topology and exact geometry (STEP export), since facade version 3.
 * f64[0] = one stream of numbers (indices and counts as exact doubles):
 *   format (1), approx_tol,
 *   n_vertices, (x y z) per vertex,
 *   n_edges, per edge: start_vertex, end_vertex, curve_class, t0, t1, approximated, CURVE
 *     (start vertex at t0; curve_class 0 B-spline, 1 line, 2 circle, 3 ellipse, 4 other,
 *      approximated to approx_tol)
 *   n_bodies, per body: kind (0 solid, 1 sheet), n_shells,
 *     per shell: inner (1 = void of the solid), n_faces,
 *       per face: face_index (as in sc_faces), tag, SURFACE,
 *                 sample_ok, px py pz, nx ny nz, u v, same  (point, normal of the written
 *                 side, NURBS parameters of the point, 1 if the written side's normal is the
 *                 surface normal),
 *                 n_loops, per loop: n_edgeuses (0 = vertex loop, then its vertex),
 *                   per edgeuse: edge, forward (1 = along the edge)
 *   Loops run counter-clockwise about the written side's normal, which for solids points
 *   out of the material (for voids: into the void).
 *   CURVE   = degree, n_ctrl, n_knots, knots..., (x y z w) per control point
 *   SURFACE = class (0 B-spline, 1 plane, 2 cylinder, 3 cone, 4 sphere, 5 torus,
 *             6 revolution, 7 extrusion, 8 other), 13 parameters: origin xyz, z axis xyz,
 *             x axis xyz, p1..p4 (cylinder/sphere: r; cone: r at origin, semi-angle (rad,
 *             may be negative); torus: R, r; extrusion: vector in the z-axis slot),
 *             approximated, du, dv, nu, nv, n_ku, ku..., n_kv, kv...,
 *             (x y z w) per control point (u-major), then for classes 6 and 7: CURVE
 *             (generator). */
SC_API int32_t sc_brep(uint32_t h);

#ifdef __cplusplus
}
#endif

#endif
