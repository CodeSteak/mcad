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
SC_API uint32_t sc_extrude(const double* loop, uint32_t len, double height, double twist,
                           double scale_x, double scale_y, uint32_t tag);
SC_API uint32_t sc_revolve(const double* loop, uint32_t len, double angle, uint32_t tag);
SC_API uint32_t sc_loft(const double* loops, uint32_t len, uint32_t n_sections, int32_t ruled,
                        uint32_t tag);
/* loop frame is ignored; path: xyz per point; interp 0 = linear, 1 = spline. */
SC_API uint32_t sc_sweep(const double* loop, uint32_t len, const double* path, uint32_t n_path,
                         int32_t interp, uint32_t tag);
/* rows x cols points, row-major, xyz each */
SC_API uint32_t sc_nurbs_surface(const double* pts, uint32_t rows, uint32_t cols, uint32_t tag);
SC_API uint32_t sc_thicken(uint32_t h, double t, uint32_t tag);
SC_API uint32_t sc_shell(uint32_t h, double t, const uint32_t* open_faces, uint32_t n,
                         uint32_t tag);

/* queries */
/* edges: f64[0] = points (xyz), u32[0] = per edge: n_points, n_tags, tags... */
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

#ifdef __cplusplus
}
#endif

#endif
