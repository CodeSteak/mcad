// controller-cad facade over SMLib / SM_API. See sc_facade.h for the conventions.
//
// Provenance
// ----------
// Every face carries its tag as an SmLongAttribute (id SC_TAG_ATTR_ID, SM_AB_COPY).
// SMLib copies such attributes on Brep copy, keeps them on faces that move between Breps
// in a Boolean and copies them to both children when a face is split (SmAttribute::Split).
// New faces (fillet/chamfer faces, offset walls, caps) come out without the attribute.
//
// After every operation the facade resolves tags (resolve_tags):
//  * a face with an attribute keeps it (booleans, transforms: no further check);
//  * for fillet, chamfer, shell and thicken the attribute is additionally verified
//    geometrically: an interior point of the result face must lie on an input face
//    with that tag, otherwise the face counts as new;
//  * a face without attribute gets the tag of the input face it lies on (closest-point
//    distance below tolerance), or the operation tag if it lies on none.

#include <SmSmlibAll.h>
#include <SmApiGeneral.h>
#include <SmApiPrimitives.h>
#include <SmApiBrep.h>
#include <SmApiFillets.h>
#include <SmApiQueries.h>
#include <SmApiCurves.h>
#include <SmApiSurfaces.h>
#include <SmApiTrimmedSurfaces.h>
#include <SmFilletExecutive.h>
#include <SmFilletStandardSolver.h>
#include <SmPoly.h>
#include <SmRegion.h>
#include <SmShell.h>
#include <SmFaceuse.h>
#include <SmEdgeuse.h>
#include <SmPointClass.h>
#include <SmMessages.h>
#include <SmNurbsSrf.h>
#include <SmSurfOfRevolution.h>
#include <SmSurfOfExtrusion.h>
#include <nurbs.h>

#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <functional>
#include <map>
#include <new>
#include <string>
#include <unordered_map>
#include <vector>

#include "sc_facade.h"

namespace {

const ULONG SC_TAG_ATTR_ID = SM_AI_USER_1 + 77;  // 10077
// 4: sc_edges reports flags (SC_EDGE_SMOOTH), sc_law_stations / sc_fillet_var (variable
//    radius), surfaces from 2 or 3 points per direction, shells from Booleans.
const uint32_t SC_FACADE_VERSION = 4;

// ------------------------------------------------------------------ errors

struct Failure {
  int32_t code;
  std::string msg;
  std::vector<uint32_t> edges;
};

int32_t g_status = SC_OK;
std::string g_message;
std::vector<uint32_t> g_err_edges;
std::string g_trail;

std::vector<double> g_f64[8];
std::vector<float> g_f32[8];
std::vector<uint32_t> g_u32[8];

const char* status_name(SmStatus s) {
  switch (s) {
    case SM_SUCCESS: return "SM_SUCCESS";
    case SM_ERR: return "SM_ERR";
    case SM_ERR_OUT_OF_MEMORY: return "SM_ERR_OUT_OF_MEMORY";
    case SM_ERR_UNKNOWN: return "SM_ERR_UNKNOWN";
    case SM_ERR_FATAL: return "SM_ERR_FATAL";
    case SM_ERR_ASSERT_FAILURE: return "SM_ERR_ASSERT_FAILURE";
    case SM_ERR_NULL_POINTER: return "SM_ERR_NULL_POINTER";
    case SM_ERR_INVALID_INPUT: return "SM_ERR_INVALID_INPUT";
    case SM_ERR_NON_NULL_OUTPUT_POINTER: return "SM_ERR_NON_NULL_OUTPUT_POINTER";
    case SM_ERR_ASSERTVALID_FAILURE: return "SM_ERR_ASSERTVALID_FAILURE";
    case SM_ERR_OUTSIDE_OF_DOMAIN: return "SM_ERR_OUTSIDE_OF_DOMAIN";
    case SM_ERR_NOT_WITHIN_TOLERANCE: return "SM_ERR_NOT_WITHIN_TOLERANCE";
    case SM_ERR_NOT_CONVERGING: return "SM_ERR_NOT_CONVERGING";
    case SM_ERR_AXIS_INSIDE_FACE: return "SM_ERR_AXIS_INSIDE_FACE";
    case SM_ERR_BAD_INTERSECTIONS: return "SM_ERR_BAD_INTERSECTIONS";
    case SM_ERR_BAD_COINCIDENT_VERTICES: return "SM_ERR_BAD_COINCIDENT_VERTICES";
    case SM_ERR_BAD_SURFACE_POINT: return "SM_ERR_BAD_SURFACE_POINT";
    case SM_ERR_BAD_TANGENT_DROP: return "SM_ERR_BAD_TANGENT_DROP";
    case SM_ERR_DEGENERATE_SURFACE: return "SM_ERR_DEGENERATE_SURFACE";
    case SM_ERR_BAD_FIND_DEGEN_PARAM: return "SM_ERR_BAD_FIND_DEGEN_PARAM";
    case SM_ERR_METHOD_FAILURE_QUITING: return "SM_ERR_METHOD_FAILURE_QUITING";
    case SM_ERR_NOTYET_HEAL_FACE: return "SM_ERR_NOTYET_HEAL_FACE";
    case SM_ERR_TESS_FAIL_FACE: return "SM_ERR_TESS_FAIL_FACE";
    default: return "SM_ERR_?";
  }
}

std::string basename_of(const std::string& p) {
  size_t i = p.find_last_of("/\\");
  return i == std::string::npos ? p : p.substr(i + 1);
}

void error_callback(SmStatus error, const TCHAR* file, ULONG line, const TCHAR* message) {
  if (error == SM_SUCCESS || error == SM_ERR_WARNING || error == SM_ERR_MESSAGE) return;
  if (g_trail.size() > 2048) return;
  std::string e = status_name(error);
  if (file && *file) {
    e += " at ";
    e += basename_of(smos_FromTChar(file));
    e += ":";
    e += std::to_string((unsigned long)line);
  }
  if (message && *message) {
    e += " (";
    e += smos_FromTChar(message);
    e += ")";
  }
  if (!g_trail.empty()) g_trail += "; ";
  g_trail += e;
  if (g_trail.size() > 2048) g_trail += " ...";
}

Failure fail(int32_t code, std::string msg, std::vector<uint32_t> edges = {}) {
  return Failure{code, std::move(msg), std::move(edges)};
}

// Failure of an SMLib call: message with status name and the collected kernel trace.
Failure sm_fail(const char* what, SmStatus st, std::vector<uint32_t> edges = {}) {
  std::string m = std::string(what) + " failed: " + status_name(st) + " (" +
                  std::to_string((long)st) + ")";
  if (!g_trail.empty()) m += "; kernel trace: " + g_trail;
  return Failure{SC_FAILED, m, std::move(edges)};
}

// ------------------------------------------------------------------ deterministic rand()
//
// SMLib seeds the C library generator with srand(time(NULL)) and draws from rand() in
// SmFace::GetPointsInFace (dithered interior sample points, used by SmShell::IsInnerShellOf
// and the Boolean face classification) and in SmShell::IsInnerShellOf (ray directions for
// wire/vertex shells). Results then depend on the wall-clock second. The facade defines
// rand()/srand() itself (they take precedence over the C library's in the static link, native
// and Emscripten): srand() ignores its argument and restarts a fixed sequence, and every sc_*
// call restarts it as well, so each operation sees the same draws regardless of time and
// call history.
const uint32_t SC_RAND_SEED = 0x2545F491u;
uint64_t g_rand_state = SC_RAND_SEED;

void rand_reset() { g_rand_state = SC_RAND_SEED; }

}  // namespace

#ifdef __THROW
#define SC_LIBC_NOTHROW __THROW
#else
#define SC_LIBC_NOTHROW
#endif

extern "C" {
void srand(unsigned) SC_LIBC_NOTHROW { rand_reset(); }
int rand(void) SC_LIBC_NOTHROW {
  // 64-bit LCG (Knuth MMIX), upper 31 bits: uniform over [0, RAND_MAX] for RAND_MAX = 2^31 - 1.
  g_rand_state = g_rand_state * 6364136223846793005ULL + 1442695040888963407ULL;
  return (int)((g_rand_state >> 33) & (uint64_t)RAND_MAX);
}
}

namespace {

bool g_inited = false;
// Draft mode (sc_set_draft): results serve a temporary display only; booleans skip the
// plausibility check while SMLib succeeds, fillets skip the self-overlap check.
bool g_draft = false;
void ensure_init() {
  if (g_inited) return;
  SmApiCreateContext();
  smos_SetErrorCallback(error_callback);
  g_inited = true;
}

void begin_call() {
  ensure_init();
  smos_SetErrorCallback(error_callback);  // cheap; covers callers on other threads
  rand_reset();
  g_status = SC_OK;
  g_message.clear();
  g_err_edges.clear();
  g_trail.clear();
  for (auto& v : g_f64) v.clear();
  for (auto& v : g_f32) v.clear();
  for (auto& v : g_u32) v.clear();
}

void set_failure(const Failure& f) {
  g_status = f.code;
  g_message = f.msg;
  g_err_edges = f.edges;
}

// Runs `body`, catching every C++ exception. `body` returns true on success or
// throws/returns a Failure through `out`.
template <class F>
bool guarded(F&& body) {
  try {
    body();
    return true;
  } catch (const Failure& f) {
    set_failure(f);
  } catch (const std::bad_alloc&) {
    set_failure(fail(SC_FAILED, "out of memory"));
  } catch (const std::exception& e) {
    set_failure(fail(SC_FAILED, std::string("C++ exception: ") + e.what()));
  } catch (...) {
    set_failure(fail(SC_FAILED, "unknown C++ exception in SMLib"));
  }
  return false;
}

#define SC_CHECK(expr, what)                    \
  do {                                          \
    SmStatus _st = (SmStatus)(expr);            \
    if (_st != SM_SUCCESS) throw sm_fail(what, _st); \
  } while (0)

// ------------------------------------------------------------------ handles

struct Entry {
  SmBrep* brep = nullptr;
  uint32_t rc = 0;
};
std::vector<Entry> g_table(1);  // slot 0 unused
std::vector<uint32_t> g_free_slots;
uint32_t g_live = 0;

// Owns a Brep until released or handed to the table.
struct BrepPtr {
  SmBrep* p = nullptr;
  BrepPtr() = default;
  explicit BrepPtr(SmBrep* b) : p(b) {}
  BrepPtr(const BrepPtr&) = delete;
  BrepPtr& operator=(const BrepPtr&) = delete;
  BrepPtr(BrepPtr&& o) noexcept : p(o.p) { o.p = nullptr; }
  BrepPtr& operator=(BrepPtr&& o) noexcept {
    reset();
    p = o.p;
    o.p = nullptr;
    return *this;
  }
  ~BrepPtr() { reset(); }
  void reset(SmBrep* b = nullptr) {
    if (p && p != b) delete p;
    p = b;
  }
  SmBrep* release() {
    SmBrep* r = p;
    p = nullptr;
    return r;
  }
  SmBrep* get() const { return p; }
  SmBrep* operator->() const { return p; }
};

template <class T>
struct ObjPtr {  // owns a standalone SMLib object (curve, surface)
  T* p = nullptr;
  ObjPtr() = default;
  explicit ObjPtr(T* o) : p(o) {}
  ObjPtr(const ObjPtr&) = delete;
  ObjPtr(ObjPtr&& o) noexcept : p(o.p) { o.p = nullptr; }
  ~ObjPtr() {
    if (p) delete p;
  }
  T* release() {
    T* r = p;
    p = nullptr;
    return r;
  }
};

struct CurveList {  // owns standalone curves
  std::vector<SmCurve*> v;
  CurveList() = default;
  CurveList(const CurveList&) = delete;
  CurveList(CurveList&& o) noexcept : v(std::move(o.v)) {}
  ~CurveList() {
    for (SmCurve* c : v)
      if (c) delete c;
  }
  void add(SmCurve* c) { v.push_back(c); }
};

uint32_t put(SmBrep* b) {
  uint32_t h;
  if (!g_free_slots.empty()) {
    h = g_free_slots.back();
    g_free_slots.pop_back();
  } else {
    h = (uint32_t)g_table.size();
    g_table.push_back(Entry{});
  }
  g_table[h].brep = b;
  g_table[h].rc = 1;
  g_live++;
  return h;
}

uint32_t put(BrepPtr&& b) { return put(b.release()); }

SmBrep* get(uint32_t h) {
  if (h == 0 || h >= g_table.size() || g_table[h].brep == nullptr)
    throw fail(SC_INVALID, "invalid shape handle " + std::to_string(h));
  return g_table[h].brep;
}

SmContext* ctx() { return SmApiGetOrCreateContext(); }

BrepPtr copy_brep(const SmBrep* b) {
  SmBrep* c = nullptr;
  SC_CHECK(SmApiBrepCopy(b, c), "SmApiBrepCopy");
  if (!c) throw fail(SC_FAILED, "SmApiBrepCopy returned no Brep");
  return BrepPtr(c);
}

BrepPtr empty_brep() { return BrepPtr(new (*ctx()) SmBrep()); }

// ------------------------------------------------------------------ small math

struct V3 {
  double x, y, z;
};
inline V3 v3(double x, double y, double z) { return V3{x, y, z}; }
inline V3 operator+(V3 a, V3 b) { return v3(a.x + b.x, a.y + b.y, a.z + b.z); }
inline V3 operator-(V3 a, V3 b) { return v3(a.x - b.x, a.y - b.y, a.z - b.z); }
inline V3 operator*(V3 a, double s) { return v3(a.x * s, a.y * s, a.z * s); }
inline double dot(V3 a, V3 b) { return a.x * b.x + a.y * b.y + a.z * b.z; }
inline V3 cross(V3 a, V3 b) {
  return v3(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x);
}
inline double len(V3 a) { return std::sqrt(dot(a, a)); }
inline V3 normalize(V3 a) {
  double l = len(a);
  return l > 0 ? a * (1.0 / l) : a;
}
inline SmPoint3d sp(V3 a) { return SmPoint3d(a.x, a.y, a.z); }
inline SmVector3d sv(V3 a) { return SmVector3d(a.x, a.y, a.z); }
inline V3 fromsm(const SmPoint3d& p) { return v3(p.x, p.y, p.z); }

// Column-major 4x4 (glam layout): m[c*4 + r].
struct M4 {
  double m[16];
  V3 col(int c) const { return v3(m[c * 4], m[c * 4 + 1], m[c * 4 + 2]); }
  V3 point(V3 p) const { return col(0) * p.x + col(1) * p.y + col(2) * p.z + col(3); }
  V3 vec(V3 p) const { return col(0) * p.x + col(1) * p.y + col(2) * p.z; }
  double det3() const { return dot(col(0), cross(col(1), col(2))); }
};

M4 identity() {
  M4 r{};
  r.m[0] = r.m[5] = r.m[10] = r.m[15] = 1.0;
  return r;
}

bool is_fin(double d) { return std::isfinite(d); }

// Is the linear part a similarity (orthogonal * uniform scale)? Returns the scale.
bool similarity(const M4& f, double& s) {
  V3 a = f.col(0), b = f.col(1), c = f.col(2);
  double la = len(a), lb = len(b), lc = len(c);
  if (la <= 0 || !is_fin(la)) return false;
  double tol = 1e-9 * la;
  if (std::fabs(la - lb) > 1e-7 * la || std::fabs(la - lc) > 1e-7 * la) return false;
  if (std::fabs(dot(a, b)) > tol * la || std::fabs(dot(a, c)) > tol * la ||
      std::fabs(dot(b, c)) > tol * la)
    return false;
  s = la;
  return true;
}

// ------------------------------------------------------------------ brep helpers

std::vector<SmFace*> faces_of(const SmBrep* b) {
  SmTArray<SmFace*> f;
  b->GetFaces(f);
  return std::vector<SmFace*>(f.GetDataArray(), f.GetDataArray() + f.GetSize());
}

std::vector<SmEdge*> edges_of(const SmBrep* b) {
  SmTArray<SmEdge*> e;
  b->GetEdges(e);
  return std::vector<SmEdge*>(e.GetDataArray(), e.GetDataArray() + e.GetSize());
}

bool face_tag(const SmFace* f, uint32_t& tag) {
  SmAttribute* a = f->FindAttribute(SC_TAG_ATTR_ID);
  if (!a || !a->IsKindOf(SmLongAttribute_TYPE)) return false;
  tag = (uint32_t)((SmLongAttribute*)a)->GetValue();
  return true;
}

void set_face_tag(SmFace* f, uint32_t tag) {
  SmAttribute* a = f->FindAttribute(SC_TAG_ATTR_ID);
  if (a) f->RemoveAttribute(a);
  f->AddAttribute(new (*ctx()) SmLongAttribute(SC_TAG_ATTR_ID, (long)tag, SM_AB_COPY));
}

void tag_all(SmBrep* b, uint32_t tag) {
  for (SmFace* f : faces_of(b)) set_face_tag(f, tag);
}

double model_diag(const SmBrep* b) {
  SmPoint3d mn, mx;
  if (b->GetNumFaces() == 0) return 1.0;
  if (SmApiBrepBoundingBox(b, FALSE, mn, mx) != SM_SUCCESS) return 1.0;
  double d = len(fromsm(mx) - fromsm(mn));
  return (d > 0 && is_fin(d)) ? d : 1.0;
}

// Point strictly inside the face and its outward normal.
bool face_sample(const SmFace* f, V3& p, V3& n, SmPoint2d* uv_out = nullptr) {
  SmSurface* srf = f->GetSurface();
  if (!srf) return false;
  SmExtent2d d = f->GetUVDomain();
  const int N = 7;
  double best = 1e300;
  SmPoint2d buv;
  bool found = false;
  for (int pass = 0; pass < 2 && !found; pass++) {
    int M = pass == 0 ? N : 23;
    for (int i = 0; i < M; i++)
      for (int j = 0; j < M; j++) {
        double s = (i + 0.5) / M, t = (j + 0.5) / M;
        SmPoint2d uv(d.GetUMin() + s * (d.GetUMax() - d.GetUMin()),
                     d.GetVMin() + t * (d.GetVMax() - d.GetVMin()));
        SmPointClassification pc;
        if (f->PointClassify(uv, SmTol::GetZoneTol3d(f), FALSE, TRUE, pc) != SM_SUCCESS)
          continue;
        if (pc.GetPointClass() != SM_PC_FACE) continue;
        double dd = (s - 0.5) * (s - 0.5) + (t - 0.5) * (t - 0.5);
        if (dd < best) {
          best = dd;
          buv = uv;
          found = true;
        }
      }
  }
  SmVector3d pv, nv;
  if (found) {
    SmVector2d uvv(buv.x, buv.y);
    if (SmApiEvaluateSurfacePoint(srf, uvv, pv) != SM_SUCCESS) return false;
    if (SmApiEvaluateSurfaceNormal(srf, uvv, nv) != SM_SUCCESS) return false;
  } else {
    // Thin face: fall back to SMLib's internal point and invert it on the surface.
    SmPoint3d ip;
    if (f->CalculateAnInternalPoint(ip) != SM_SUCCESS) return false;
    SmPoint3d cp;
    SmTArray<SmPoint2d> uvs;
    double dist;
    if (SmApiSurfaceClosestPoint(srf, ip, cp, uvs, dist) != SM_SUCCESS || uvs.GetSize() == 0)
      return false;
    buv = uvs[0];
    SmVector2d uvv(buv.x, buv.y);
    pv.Set(ip.x, ip.y, ip.z);
    if (SmApiEvaluateSurfaceNormal(srf, uvv, nv) != SM_SUCCESS) return false;
  }
  p = v3(pv.x, pv.y, pv.z);
  n = normalize(v3(nv.x, nv.y, nv.z));
  SmFaceuse* fu = f->GetUpwardFaceuse();
  if (fu && fu->GetShell() && fu->GetShell()->GetRegion() && !fu->GetShell()->GetRegion()->IsVoid())
    n = n * -1.0;
  if (uv_out) *uv_out = buv;
  return true;
}

// 0 = outside, 1 = inside, 2 = on boundary, -1 = error
int classify_point(const SmBrep* b, V3 p) {
  if (b->GetNumFaces() == 0) return 0;
  SmPointClassification pc;
  if (b->Point3DClassify(sp(p), SmTol::GetZoneTol3d(b), TRUE, pc) != SM_SUCCESS) return -1;
  SmPointClassificationType t = pc.GetPointClass();
  if (t == SM_PC_REGION) {
    SmRegion* r = (SmRegion*)pc.GetObject();
    return (r && !r->IsVoid()) ? 1 : 0;
  }
  if (t == SM_PC_FACE || t == SM_PC_EDGE || t == SM_PC_VERTEX) return 2;
  return 0;
}

// Local consistency of a solid: just outside every face must be outside, just inside must be
// inside. Catches fillets/shells that SMLib reports as successful but that overlap themselves.
// Returns the indices of offending faces.
std::vector<size_t> invalid_faces(const SmBrep* b) {
  std::vector<size_t> bad;
  if (!b->IsManifoldSolid()) return bad;  // only meaningful for closed solids
  double eps = 1e-4 * model_diag(b);
  auto fs = faces_of(b);
  for (size_t i = 0; i < fs.size(); i++) {
    V3 p, n;
    if (!face_sample(fs[i], p, n)) continue;
    int out = classify_point(b, p + n * eps);
    int in = classify_point(b, p - n * eps);
    if (out < 0 || in < 0) continue;
    if (out != 0 || in != 1) bad.push_back(i);
  }
  return bad;
}

double face_distance(SmFace* f, V3 p) {
  SmPoint3d cp;
  double d = 1e300;
  if (SmApiFaceClosestPoint(f, sp(p), cp, d) != SM_SUCCESS) return 1e300;
  return d;
}

struct FaceBox {
  SmFace* f;
  uint32_t tag;
  bool has_tag;
  V3 mn, mx;
};

std::vector<FaceBox> face_boxes(const std::vector<const SmBrep*>& inputs,
                                const std::vector<const SmFace*>& exclude) {
  std::vector<FaceBox> out;
  for (const SmBrep* b : inputs) {
    for (SmFace* f : faces_of(b)) {
      if (std::find(exclude.begin(), exclude.end(), f) != exclude.end()) continue;
      FaceBox fb{f, 0, false, v3(0, 0, 0), v3(0, 0, 0)};
      fb.has_tag = face_tag(f, fb.tag);
      SmPoint3d mn, mx;
      if (SmApiFaceBoundingBox(f, FALSE, mn, mx) == SM_SUCCESS) {
        fb.mn = fromsm(mn);
        fb.mx = fromsm(mx);
      } else {
        fb.mn = v3(-1e300, -1e300, -1e300);
        fb.mx = v3(1e300, 1e300, 1e300);
      }
      out.push_back(fb);
    }
  }
  return out;
}

// See "Provenance" at the top of this file.
size_t resolve_tags(SmBrep* result, const std::vector<const SmBrep*>& inputs, uint32_t op_tag,
                    bool verify, const std::vector<const SmFace*>& exclude = {}) {
  size_t fresh = 0;
  std::vector<FaceBox> boxes;
  bool boxes_ready = false;
  double tol = 1e-6;
  for (const SmBrep* b : inputs) tol = std::max(tol, 1e-6 * model_diag(b));
  tol = std::max(tol, 1e-6 * model_diag(result));

  for (SmFace* f : faces_of(result)) {
    uint32_t t = 0;
    bool has = face_tag(f, t);
    if (has && !verify) continue;
    V3 p, n;
    if (!face_sample(f, p, n)) {
      if (!has) {
        set_face_tag(f, op_tag);
        fresh++;
      }
      continue;
    }
    if (!boxes_ready) {
      boxes = face_boxes(inputs, exclude);
      boxes_ready = true;
    }
    double best = 1e300;
    uint32_t best_tag = op_tag;
    bool attr_confirmed = false;
    bool on_input = false;
    for (const FaceBox& fb : boxes) {
      if (!fb.has_tag) continue;
      if (p.x < fb.mn.x - tol || p.y < fb.mn.y - tol || p.z < fb.mn.z - tol ||
          p.x > fb.mx.x + tol || p.y > fb.mx.y + tol || p.z > fb.mx.z + tol)
        continue;
      double d = face_distance(fb.f, p);
      if (d > tol) continue;
      on_input = true;
      if (has && fb.tag == t) {
        attr_confirmed = true;
        break;
      }
      if (d < best) {
        best = d;
        best_tag = fb.tag;
      }
    }
    if (attr_confirmed) continue;
    if (!on_input) fresh++;
    set_face_tag(f, best_tag);
  }
  return fresh;
}

// ------------------------------------------------------------------ profiles

struct Loop {
  int kind = 0;
  std::vector<V3> pts;  // 2D points (z = 0) for polygon/spline
  double cx = 0, cy = 0, r = 0;
  M4 frame = identity();
  std::vector<double> sk;  // composite (sketch) segments, raw; see sc_sketch.cpp
};

// sketches (sc_sketch.cpp, included at the end of this file)
uint32_t sketch_parse(const double* d, uint32_t i, uint32_t dlen, uint32_t count, Loop& l);
void sketch_curves(const Loop& l, const M4& f, bool single, CurveList& out);
bool loft_segmentwise(const std::vector<Loop>& loops);

std::vector<Loop> parse_loops(const double* d, uint32_t dlen, uint32_t expect) {
  std::vector<Loop> out;
  uint32_t i = 0;
  while (i < dlen) {
    if (i + 2 > dlen) throw fail(SC_INVALID, "truncated loop data");
    Loop l;
    l.kind = (int)d[i];
    uint32_t count = (uint32_t)d[i + 1];
    i += 2;
    if (l.kind == SC_LOOP_POLYGON || l.kind == SC_LOOP_SPLINE) {
      if (i + 2 * count + 16 > dlen) throw fail(SC_INVALID, "truncated loop data");
      for (uint32_t k = 0; k < count; k++) {
        V3 p = v3(d[i + 2 * k], d[i + 2 * k + 1], 0);
        if (!is_fin(p.x) || !is_fin(p.y)) throw fail(SC_INVALID, "non-finite profile point");
        if (!l.pts.empty() && len(p - l.pts.back()) < 1e-12) continue;
        l.pts.push_back(p);
      }
      while (l.pts.size() > 1 && len(l.pts.front() - l.pts.back()) < 1e-12) l.pts.pop_back();
      i += 2 * count;
      if (l.pts.size() < 3) throw fail(SC_INVALID, "profile needs at least 3 distinct points");
    } else if (l.kind == SC_LOOP_CIRCLE) {
      if (count != 3 || i + 3 + 16 > dlen) throw fail(SC_INVALID, "bad circle loop data");
      l.cx = d[i];
      l.cy = d[i + 1];
      l.r = d[i + 2];
      i += 3;
      if (!(l.r > 0) || !is_fin(l.r)) throw fail(SC_INVALID, "circle radius must be > 0");
    } else if (l.kind == SC_LOOP_COMPOSITE) {
      i = sketch_parse(d, i, dlen, count, l);
    } else {
      throw fail(SC_INVALID, "unknown loop kind " + std::to_string(l.kind));
    }
    std::memcpy(l.frame.m, d + i, sizeof(double) * 16);
    i += 16;
    for (double v : l.frame.m)
      if (!is_fin(v)) throw fail(SC_INVALID, "non-finite profile frame");
    out.push_back(std::move(l));
  }
  if (expect && out.size() != expect)
    throw fail(SC_INVALID, "expected " + std::to_string(expect) + " loops, got " +
                               std::to_string(out.size()));
  return out;
}

SmBSplineCurve* polyline_curve(const std::vector<V3>& pts, bool closed) {
  // degree-1 B-spline through the points (clamped, chord-length knots)
  std::vector<V3> p = pts;
  if (closed) p.push_back(pts.front());
  SmTArray<SmPoint3d> cp;
  for (V3 q : p) cp.Add(sp(q));
  SmTArray<double> knots;
  SmTArray<ULONG> mult;
  double t = 0;
  for (size_t i = 0; i < p.size(); i++) {
    if (i > 0) t += std::max(len(p[i] - p[i - 1]), 1e-12);
    knots.Add(t);
    mult.Add((i == 0 || i + 1 == p.size()) ? 2 : 1);
  }
  SmBSplineCurve* c = nullptr;
  SC_CHECK(SmApiCreateCanonicalCurve(cp, knots, mult, nullptr, 1, SM_CF_POLYLINE_FORM, c),
           "SmApiCreateCanonicalCurve");
  return c;
}

SmCurve* interp_curve(const std::vector<V3>& pts, bool closed) {
  std::vector<V3> p = pts;
  if (closed) p.push_back(pts.front());
  if (!closed && p.size() == 3) {
    // SMLib interpolates cubics through >= 4 points: add two points on the parabola through
    // the three (chord-length parameters).
    double t1 = len(p[1] - p[0]), t2 = t1 + len(p[2] - p[1]);
    auto q = [&](double t) {
      double l0 = (t - t1) * (t - t2) / ((0 - t1) * (0 - t2));
      double l1 = (t - 0) * (t - t2) / ((t1 - 0) * (t1 - t2));
      double l2 = (t - 0) * (t - t1) / ((t2 - 0) * (t2 - t1));
      return p[0] * l0 + p[1] * l1 + p[2] * l2;
    };
    p = {p[0], q(t1 / 2), p[1], q((t1 + t2) / 2), p[2]};
  }
  if (p.size() < 4) return polyline_curve(pts, closed);
  SmTArray<SmPoint3d> cp;
  for (V3 q : p) cp.Add(sp(q));
  SmCurve* c = nullptr;
  SC_CHECK(SmApiCreateCurveInterpPoints(cp, c, SM_CP_CHORDLENGTH), "SmApiCreateCurveInterpPoints");
  return c;
}

void place_curve(SmCurve* c, const M4& f, double s) {
  V3 x = normalize(f.col(0)), y = normalize(f.col(1));
  SmAxis2Placement pl(sp(f.col(3)), sv(x), sv(y));
  SmVector3d sc(s, s, s);
  SC_CHECK(c->Transform(pl, s == 1.0 ? nullptr : &sc), "SmCurve::Transform");
}

// Curves of a loop in model space (frame applied unless `local`).
// single = one closed curve per loop (for lofts); otherwise polygon edges are separate lines
// and circles are analytic.
void loop_curves(const Loop& l, bool single, bool local, CurveList& out) {
  M4 f = local ? identity() : l.frame;
  if (l.kind == SC_LOOP_COMPOSITE) {
    sketch_curves(l, f, single, out);
    return;
  }
  if (l.kind == SC_LOOP_CIRCLE) {
    double s;
    if (!similarity(f, s))
      throw fail(SC_UNSUPPORTED, "circle profile under a non-uniform frame (ellipse)");
    SmCurve* c = nullptr;
    if (single) {
      SmBSplineCurve* b = nullptr;
      SC_CHECK(SmApiCreateArc(SmPoint3d(0, 0, 0), l.r, 0.0, 360.0, b), "SmApiCreateArc");
      c = b;
    } else {
      SmCircle* ci = nullptr;
      SC_CHECK(SmApiCreateCircle(SmPoint3d(0, 0, 0), l.r, ci), "SmApiCreateCircle");
      c = ci;
    }
    out.add(c);
    M4 g = f;
    // move the circle centre: frame * translate(cx, cy)
    V3 o = f.point(v3(l.cx, l.cy, 0));
    g.m[12] = o.x;
    g.m[13] = o.y;
    g.m[14] = o.z;
    place_curve(c, g, s);
    return;
  }
  std::vector<V3> pts;
  for (V3 p : l.pts) pts.push_back(f.point(p));
  if (l.kind == SC_LOOP_SPLINE) {
    out.add(interp_curve(pts, true));
    return;
  }
  if (single) {
    out.add(polyline_curve(pts, true));
    return;
  }
  for (size_t i = 0; i < pts.size(); i++) {
    SmLine* ln = nullptr;
    SC_CHECK(SmApiCreateLineSegment(sp(pts[i]), sp(pts[(i + 1) % pts.size()]), ln),
             "SmApiCreateLineSegment");
    out.add(ln);
  }
}

// Profile curves of a revolve (model space, axis = z through the origin, radial = x).
//
// A closed interpolating spline cannot pass through two points on the axis without crossing
// it: at each axis point it has a tangent with a radial component, so it overshoots to x < 0
// next to it (knauf_mirror: the spline (0,0) (r,0) ... (r,h) (0,h) bulges to x = -1.6 between
// (0,h) and (0,0)). SMLib's full-turn sweep does not check that and returns a self-overlapping
// solid (wrong volume, open booleans, misclassified points, failed fillets). For a revolve, two
// spline points that follow each other (cyclically) on the axis are therefore joined by a
// straight segment along the axis, and the spline runs open between such segments. Afterwards
// every profile curve is sampled: one reaching x < 0 fails with SC_INVALID.
void revolve_profile_curves(const Loop& l, CurveList& out) {
  if (l.kind != SC_LOOP_SPLINE) {
    loop_curves(l, false, false, out);
  } else {
    std::vector<V3> pts;
    double ext = 0;
    for (V3 p : l.pts) pts.push_back(l.frame.point(p));
    for (V3 p : pts) ext = std::max(ext, len(p));
    double atol = 1e-9 * std::max(1.0, ext);
    size_t n = pts.size();
    auto on_axis = [&](size_t i) { return std::hypot(pts[i].x, pts[i].y) <= atol; };
    std::vector<size_t> cuts;  // i: segment i -> i+1 lies on the axis
    for (size_t i = 0; i < n; i++)
      if (on_axis(i) && on_axis((i + 1) % n)) cuts.push_back(i);
    if (cuts.empty()) {
      loop_curves(l, false, false, out);
    } else {
      for (size_t c = 0; c < cuts.size(); c++) {
        size_t a = cuts[c], b = (a + 1) % n;
        SmLine* ln = nullptr;
        SC_CHECK(SmApiCreateLineSegment(sp(pts[a]), sp(pts[b]), ln), "SmApiCreateLineSegment");
        out.add(ln);
        // open run from b to the start of the next axis segment
        size_t e = cuts[(c + 1) % cuts.size()];
        std::vector<V3> run{pts[b]};
        for (size_t i = b; i != e;) {
          i = (i + 1) % n;
          run.push_back(pts[i]);
        }
        if (run.size() >= 2) out.add(interp_curve(run, false));
      }
    }
  }
  double ext = 0;
  for (SmCurve* c : out.v) {
    SmExtent3d cb;
    c->CalculateBoundingBox(c->GetNaturalInterval(), &cb);
    ext = std::max({ext, len(fromsm(cb.GetMin())), len(fromsm(cb.GetMax()))});
  }
  double tol = 1e-7 * std::max(1.0, ext);
  for (SmCurve* c : out.v) {
    SmExtent1d iv = c->GetNaturalInterval();
    const int K = 256;
    for (int k = 0; k <= K; k++) {
      SmPoint3d p;
      if (c->EvaluatePoint(iv.GetMin() + (iv.GetMax() - iv.GetMin()) * k / K, p) != SM_SUCCESS)
        continue;
      if (p.x < -tol)
        throw fail(SC_INVALID, "revolve: the profile curve crosses the axis (reaches x = " +
                                   std::to_string(p.x) + " after its frame)");
    }
  }
}

SmTArray<SmCurve*> as_array(const CurveList& cl) {
  SmTArray<SmCurve*> a;
  for (SmCurve* c : cl.v) a.Add(c);
  return a;
}

void apply_matrix(SmBrep* b, const M4& m) {
  double t[4][4];
  for (int c = 0; c < 4; c++)
    for (int r = 0; r < 4; r++) t[c][r] = m.m[c * 4 + r];
  // SMLib uses row vectors: row i = image of basis vector i, row 3 = translation.
  if (std::fabs(m.det3()) < 1e-18) throw fail(SC_INVALID, "singular transform");
  // Tolerances: SmBrep::Transform(t, TRUE) re-derives the model size from the box of the
  // vertices, sets every vertex/edge/face tolerance from it and also overwrites the context's
  // model size. A torus or a revolved circle has a single vertex: its box has size 0, all
  // tolerances become 0 and every later Boolean with the body silently misses the
  // intersections (translate([0, 0, 2]) torus(20, 1) united with a plate came back as two
  // overlapping bodies, intersected with it as empty). Rigid motions keep the tolerances;
  // scaled bodies get them from their full (loose) box, the context is left alone.
  double s = 1.0;
  bool rigid = similarity(m, s) && std::fabs(s - 1.0) <= 1e-12;
  SC_CHECK(b->Transform(t, FALSE), "SmBrep::Transform");
  if (!rigid && b->GetNumFaces() > 0) {
    SmPoint3d mn, mx;
    if (SmApiBrepBoundingBox(b, FALSE, mn, mx) == SM_SUCCESS) {
      double size = len(fromsm(mx) - fromsm(mn));
      if (size > 0 && is_fin(size)) {
        SmTol::SetModelSizeEstimate(*b, size);
        SmZoneTol3d zt = SmTol::GetZoneTol3d(b);
        SmTArray<SmVertex*> vs;
        b->GetVertices(vs);
        for (ULONG i = 0; i < vs.GetSize(); i++) vs[i]->SetTolerance(zt, FALSE, FALSE);
        for (SmEdge* e : edges_of(b)) e->SetTolerance(zt, FALSE, FALSE);
        for (SmFace* f : faces_of(b)) f->SetTolerance(zt, FALSE, FALSE);
      }
    }
  }
}

bool is_identity(const M4& m) {
  M4 i = identity();
  for (int k = 0; k < 16; k++)
    if (std::fabs(m.m[k] - i.m[k]) > 1e-15) return false;
  return true;
}

void require_result(SmBrep* b, const char* what) {
  if (!b) throw fail(SC_FAILED, std::string(what) + " produced no result");
}

void require_solid(SmBrep* b, const char* what) {
  require_result(b, what);
  if (b->GetNumFaces() == 0 || !b->IsManifoldSolid())
    throw fail(SC_FAILED, std::string(what) +
                              " did not produce a closed solid (self-intersecting or open profile?)");
  double v = 0;
  if (SmApiBrepComputeVolume(b, 1e-3, v) != SM_SUCCESS || !(v > 0))
    throw fail(SC_FAILED, std::string(what) + " produced an invalid solid (volume " +
                              std::to_string(v) + ")");
}

void check_valid(SmBrep* b, const char* what, std::vector<uint32_t> edges = {}) {
  auto bad = invalid_faces(b);
  if (!bad.empty())
    throw fail(SC_FAILED,
               std::string(what) + " produced an invalid (self-overlapping) solid at " +
                   std::to_string(bad.size()) + " face(s); parameter too large?",
               std::move(edges));
}

// ------------------------------------------------------------------ extrude via loft helper

BrepPtr skin(const std::vector<CurveList*>& sections, ULONG degree, const char* what) {
  SmTArray<ULONG> counts;
  SmTArray<const SmCurve*> all;
  for (CurveList* s : sections) {
    counts.Add((ULONG)s->v.size());
    for (SmCurve* c : s->v) all.Add(c);
  }
  SmBrep* r = nullptr;
  SC_CHECK(SmApiCreateSkinPrimitive(counts, all, degree, TRUE, r, nullptr, nullptr, nullptr),
           what);
  require_result(r, what);
  return BrepPtr(r);
}

// ------------------------------------------------------------------ fillet / chamfer

std::vector<SmEdge*> select_edges(const SmBrep* b, const uint32_t* ids, uint32_t n,
                                  std::vector<SmEdge*>& all) {
  all = edges_of(b);
  std::vector<SmEdge*> sel;
  if (n == 0) throw fail(SC_INVALID, "no edges given");
  for (uint32_t i = 0; i < n; i++) {
    if (ids[i] >= all.size())
      throw fail(SC_INVALID, "edge id " + std::to_string(ids[i]) + " out of range (shape has " +
                                 std::to_string(all.size()) + " edges)",
                 {ids[i]});
    if (std::find(sel.begin(), sel.end(), all[ids[i]]) == sel.end()) sel.push_back(all[ids[i]]);
  }
  return sel;
}

uint32_t edge_index(const std::vector<SmEdge*>& all, const SmEdge* e) {
  for (size_t i = 0; i < all.size(); i++)
    if (all[i] == e) return (uint32_t)i;
  return UINT32_MAX;
}

// Requested edges near any of the given result faces (for error reporting).
std::vector<uint32_t> edges_near_faces(SmBrep* result, const std::vector<size_t>& bad_faces,
                                       const std::vector<SmEdge*>& sel_in_input,
                                       const std::vector<SmEdge*>& all_input, double reach) {
  std::vector<uint32_t> out;
  auto fs = faces_of(result);
  std::vector<V3> pts;
  for (size_t i : bad_faces) {
    V3 p, n;
    if (face_sample(fs[i], p, n)) pts.push_back(p);
  }
  for (SmEdge* e : sel_in_input) {
    for (V3 p : pts) {
      SmPoint3d cp;
      double t, d;
      if (SmApiEdgeClosestPoint(e, sp(p), cp, t, d) == SM_SUCCESS && d <= reach) {
        out.push_back(edge_index(all_input, e));
        break;
      }
    }
  }
  return out;
}

// ------------------------------------------------------------------ variable radius
//
// A variable radius along an edge is a law r(t) over the edge's normalised parameter
// t = (u - u0) / (u1 - u0) (u0, u1 the edge's interval; t = 0 at the edge's start vertex): a
// clamped B-spline of degree p = min(3, m - 1) with m control radii, uniform interior knots on
// [0, 1]. Its control radius j sits at the Greville abscissa t_j (mean of the p knots after j);
// sc_law_stations returns the edge points there, the caller evaluates its radius function at
// them and passes the values to sc_fillet_var. The spline lies between the smallest and the
// largest control radius (variation diminishing), reproduces constant and linear laws exactly
// and takes the first and last value at the edge's ends. It becomes an SmBSplineFilletLaw
// whose curve carries the radius as x and t itself as y (control points (r_j, t_j): y(t) = t),
// so SMLib can extend the law past the edge's ends, which a round needs at a corner.

std::vector<double> law_knots(uint32_t m, uint32_t& p) {
  p = std::min<uint32_t>(3, m - 1);
  std::vector<double> k;
  for (uint32_t i = 0; i <= p; i++) k.push_back(0.0);
  uint32_t inner = m - p - 1;
  for (uint32_t i = 1; i <= inner; i++) k.push_back((double)i / (double)(inner + 1));
  for (uint32_t i = 0; i <= p; i++) k.push_back(1.0);
  return k;
}

std::vector<double> law_greville(uint32_t m) {
  uint32_t p = 0;
  std::vector<double> k = law_knots(m, p);
  std::vector<double> g;
  for (uint32_t j = 0; j < m; j++) {
    double s = 0;
    for (uint32_t i = 1; i <= p; i++) s += k[j + i];
    g.push_back(s / p);
  }
  return g;
}

// Owns the laws of one variable fillet (the solvers keep references until DoFillet ends).
struct LawList {
  std::vector<SmFilletLaw*> v;
  LawList() = default;
  LawList(const LawList&) = delete;
  ~LawList() {
    for (SmFilletLaw* l : v)
      if (l) delete l;
  }
};

SmBSplineFilletLaw* make_law(const std::vector<double>& radii) {
  uint32_t m = (uint32_t)radii.size(), p = 0;
  std::vector<double> k = law_knots(m, p);
  std::vector<double> g = law_greville(m);
  SmTArray<SmPoint3d> cps;
  for (uint32_t j = 0; j < m; j++) cps.Add(SmPoint3d(radii[j], g[j], 0.0));
  SmTArray<double> unique;
  SmTArray<ULONG> mults;
  for (double x : k) {
    if (unique.GetSize() > 0 && unique[unique.GetSize() - 1] == x)
      mults[mults.GetSize() - 1] += 1;
    else {
      unique.Add(x);
      mults.Add(1);
    }
  }
  SmBSplineCurve* c = nullptr;
  SC_CHECK(SmBSplineCurve::CreateCanonical(*ctx(), 3, p, cps, SM_CF_UNSPECIFIED, mults, unique,
                                           SM_KT_UNSPECIFIED, nullptr, nullptr, c),
           "SmBSplineCurve::CreateCanonical (radius law)");
  if (!c) throw fail(SC_FAILED, "radius law could not be built");
  return new (*ctx()) SmBSplineFilletLaw(c, SmExtent1d(0.0, 1.0));
}

double solid_volume(const SmBrep* b);

// Length of an edge (polyline through 33 points of its curve).
double edge_length(const SmEdge* e) {
  SmCurve* c = e->GetCurve();
  SmExtent1d iv = e->GetInterval();
  double l = 0;
  V3 prev{};
  for (int k = 0; c && k <= 32; k++) {
    SmPoint3d p;
    if (c->EvaluatePoint(iv.GetMin() + (iv.GetMax() - iv.GetMin()) * k / 32.0, p) != SM_SUCCESS)
      return l;
    if (k > 0) l += len(fromsm(p) - prev);
    prev = fromsm(p);
  }
  return l;
}

// Groups of selected edges connected through shared vertices (indices into `sel`).
std::vector<std::vector<size_t>> edge_chains(const std::vector<SmEdge*>& sel) {
  std::vector<size_t> parent(sel.size());
  for (size_t i = 0; i < sel.size(); i++) parent[i] = i;
  std::function<size_t(size_t)> root = [&](size_t i) {
    return parent[i] == i ? i : (parent[i] = root(parent[i]));
  };
  for (size_t i = 0; i < sel.size(); i++)
    for (size_t j = i + 1; j < sel.size(); j++) {
      SmVertex* a[2] = {sel[i]->GetStartVertex(), sel[i]->GetEndVertex()};
      SmVertex* b[2] = {sel[j]->GetStartVertex(), sel[j]->GetEndVertex()};
      bool shared = false;
      for (SmVertex* x : a)
        for (SmVertex* y : b) shared |= x && x == y;
      if (shared) parent[root(i)] = root(j);
    }
  std::vector<std::vector<size_t>> out;
  std::map<size_t, size_t> slot;
  for (size_t i = 0; i < sel.size(); i++) {
    size_t r = root(i);
    if (!slot.count(r)) {
      slot[r] = out.size();
      out.emplace_back();
    }
    out[slot[r]].push_back(i);
  }
  return out;
}

// Three points of an edge (ends and middle of its interval).
bool edge_points(const SmEdge* e, V3 out[3]) {
  SmCurve* c = e->GetCurve();
  if (!c) return false;
  SmExtent1d iv = e->GetInterval();
  for (int k = 0; k < 3; k++) {
    SmPoint3d p;
    if (c->EvaluatePoint(iv.GetMin() + (iv.GetMax() - iv.GetMin()) * k / 2.0, p) != SM_SUCCESS)
      return false;
    out[k] = fromsm(p);
  }
  return true;
}

// The edge of `b` with the same ends and middle as `e` (either direction), or nullptr.
SmEdge* matching_edge(const SmBrep* b, const SmEdge* e, double tol) {
  V3 p[3];
  if (!edge_points(e, p)) return nullptr;
  for (SmEdge* x : edges_of(b)) {
    V3 q[3];
    if (!edge_points(x, q)) continue;
    bool fwd = len(p[0] - q[0]) <= tol && len(p[2] - q[2]) <= tol;
    bool rev = len(p[0] - q[2]) <= tol && len(p[2] - q[0]) <= tol;
    if ((fwd || rev) && len(p[1] - q[1]) <= tol) return x;
  }
  return nullptr;
}

// One fillet/chamfer of the edges `src_sel` of `src` (blendable edges; `orig_ids` are their ids
// in the caller's input, used in error reports and to look up `laws`). The result is checked:
//  * no self-overlapping faces (invalid_faces);
//  * a closed solid if `src` is one: SMLib sometimes leaves faces out and still returns
//    SM_SUCCESS (chamfer of the top edges of a box with rounded corners: an open shell of 26
//    faces, volume 0);
//  * the volume changes by at most what a blend of that size can change near the edges
//    (20 r^2 per unit of edge length, i.e. wedges down to ~6 degrees, plus 10 r^3 per edge,
//    plus 2 % for the coarse volumes): union(fillet) of a rounded shell and a dome came back as
//    the dome alone.
BrepPtr blend_core(const SmBrep* src, const std::vector<SmEdge*>& src_sel,
                   const std::vector<uint32_t>& orig_ids, double r, bool chamfer,
                   const std::unordered_map<uint32_t, std::vector<double>>* laws) {
  const char* what = chamfer ? "chamfer" : "fillet";
  std::vector<SmEdge*> src_all = edges_of(src);
  BrepPtr b = copy_brep(src);
  std::vector<SmEdge*> all = edges_of(b.get());
  if (all.size() != src_all.size()) throw fail(SC_FAILED, "Brep copy changed the edge count");
  std::vector<uint32_t> src_idx;  // per selected edge: its index in src_all (= in all)
  SmTArray<SmEdge*> sel;
  for (SmEdge* e : src_sel) {
    uint32_t i = edge_index(src_all, e);
    if (i == UINT32_MAX) throw fail(SC_FAILED, "edge not in the body");
    src_idx.push_back(i);
    sel.Add(all[i]);
  }
  auto orig_of_index = [&](uint32_t i) -> uint32_t {
    for (size_t k = 0; k < src_idx.size(); k++)
      if (src_idx[k] == i) return orig_ids[k];
    return UINT32_MAX;
  };

  SmFilletExecutive ex(*ctx(), b.get());
  ex.SetDoGlobalMerge(TRUE);
  SmStatus st;
  LawList owned;
  if (!laws) {
    st = ex.SetFilletParameters(b.get(), sel, chamfer ? SM_FSG_LINEAR : SM_FSG_CIRCULAR,
                                SM_FS_CONST_RADIUS, r, nullptr, -1.0, -1.0, -1.0, -1.0, 1, 1.0);
  } else {
    // per edge: constant solver where all its radii agree, the variable one otherwise
    SmTArray<SmFilletSurfaceGeneratorType> xs;
    SmTArray<SmFilletSolverType> types;
    SmTArray<double> values;
    SmTArray<SmFilletLaw*> law_ptrs;
    for (uint32_t i : orig_ids) {
      auto it = laws->find(i);
      if (it == laws->end() || it->second.empty())
        throw fail(SC_INVALID, "no radius law for edge " + std::to_string(i), {i});
      const std::vector<double>& rs = it->second;
      double lo = *std::min_element(rs.begin(), rs.end());
      double hi = *std::max_element(rs.begin(), rs.end());
      xs.Add(SM_FSG_CIRCULAR);
      if (rs.size() == 1 || hi - lo <= 1e-12 * hi) {
        types.Add(SM_FS_CONST_RADIUS);
        values.Add(rs[0]);
        law_ptrs.Add(nullptr);
      } else {
        SmBSplineFilletLaw* l = make_law(rs);
        owned.v.push_back(l);
        types.Add(SM_FS_VARIABLE_RADIUS);
        values.Add(hi);
        law_ptrs.Add(l);
      }
    }
    st = ex.SetFilletParameters(b.get(), sel, xs, types, values, &law_ptrs, -1.0, -1.0, -1.0,
                                -1.0, 1, 1.0);
  }
  // (SetFilletParameters resets the merge options to topology insertion in both cases, so the
  // SetDoGlobalMerge above has no effect; kept as SmApiFilletEdges has it.)
  if (st == SM_SUCCESS) {
    if (chamfer) {
      st = ex.CreateFilletCorners();
      if (st == SM_SUCCESS) {
        SmTArray<SmFilletCorner*>& corners = ex.GetFilletCorners();
        for (ULONG j = 0; j < corners.GetSize(); j++) corners[j]->SetBevel(TRUE);
        st = ex.DoFilleting();
      }
    } else {
      st = ex.DoFillet();
    }
  }
  if (st != SM_SUCCESS) {
    std::vector<uint32_t> bad;
    SmFilletErrorInfo* info = ex.GetFilletErrorInfo();
    if (info && info->m_pFilletedEdge) {
      uint32_t i = orig_of_index(edge_index(all, info->m_pFilletedEdge));
      if (i != UINT32_MAX) bad.push_back(i);
    }
    // smlib/patches/0002: a vertex for whose meeting edges SMLib has no corner type
    if (info && info->GetErrorCode() == SM_FILERR_VERTEX_PROBLEM &&
        smos_FromTChar(info->m_cComment).find("unsupported fillet corner") != std::string::npos)
      throw fail(SC_FAILED,
                 std::string(what) +
                     " failed: SMLib cannot blend the corner where these edges meet (faces that "
                     "touch tangentially there?)",
                 bad);
    throw sm_fail(what, st, bad);
  }
  b->StitchAndOrient();
  if (b->GetNumFaces() == 0) throw fail(SC_FAILED, std::string(what) + " removed all faces");
  auto badf = g_draft ? std::vector<size_t>() : invalid_faces(b.get());
  if (!badf.empty()) {
    std::vector<uint32_t> e;
    for (uint32_t i : edges_near_faces(b.get(), badf, src_sel, src_all, 3.0 * r + 1e-6)) {
      uint32_t o = orig_of_index(i);
      if (o != UINT32_MAX) e.push_back(o);
    }
    if (e.empty()) e = orig_ids;
    throw fail(SC_FAILED,
               std::string(what) + " failed: the result overlaps itself (size " +
                   std::to_string(r) + " too large for the adjacent faces?)",
               e);
  }
  bool src_solid = src->GetNumFaces() > 0 && src->IsManifoldSolid();
  if (src_solid && !b->IsManifoldSolid())
    throw fail(SC_FAILED,
               std::string(what) +
                   " failed: SMLib returned a body that is not closed (faces missing): the blends "
                   "of neighbouring edges collide, size too large for the wall between them?",
               orig_ids);
  if (src_solid && !g_draft) {
    double v0 = solid_volume(src), v1 = solid_volume(b.get());
    double reach = 0;
    for (SmEdge* e : src_sel) reach += 20.0 * r * r * edge_length(e) + 10.0 * r * r * r;
    if (is_fin(v0) && is_fin(v1) &&
        (v1 <= 1e-9 * std::fabs(v0) || std::fabs(v1 - v0) > reach + 2e-2 * std::fabs(v0)))
      throw fail(SC_FAILED,
                 std::string(what) + " failed: SMLib returned a wrong body (volume " +
                     std::to_string(v1) + " instead of about " + std::to_string(v0) + ")",
                 orig_ids);
  }
  return b;
}

// `laws` (variable fillet): per selected edge id its control radii; empty: radius r everywhere.
//
// If the blend of all edges fails and the edges form several separate chains (no shared
// vertex), the chains are blended one after the other, each chain's edges found again in the
// intermediate body by their geometry. That helps where SMLib fails on the whole selection but
// not on its parts (outer and inner top edges of a box with rounded corners). Variable radii
// always go in one call.
uint32_t do_fillet_like(uint32_t h, const uint32_t* ids, uint32_t n, double r, uint32_t tag,
                        bool chamfer,
                        const std::unordered_map<uint32_t, std::vector<double>>* laws = nullptr) {
  const char* what = chamfer ? "chamfer" : "fillet";
  if (laws) {
    r = 0;
    for (const auto& kv : *laws)
      for (double x : kv.second) {
        if (!(x > 0) || !is_fin(x)) throw fail(SC_INVALID, "fillet radius must be > 0", {kv.first});
        r = std::max(r, x);
      }
  }
  if (!(r > 0) || !is_fin(r)) throw fail(SC_INVALID, std::string(what) + " size must be > 0");
  SmBrep* in = get(h);
  std::vector<SmEdge*> in_all;
  std::vector<SmEdge*> in_sel = select_edges(in, ids, n, in_all);
  std::vector<SmEdge*> kept;
  std::vector<uint32_t> sel_ids;
  std::vector<uint32_t> skipped;
  for (SmEdge* e : in_sel) {
    uint32_t i = edge_index(in_all, e);
    // GetBlendEdgeuse() is NULL for edges SMLib cannot blend: seams and other
    // tangent-continuous edges, lamina edges. Nothing to round there: skip them.
    if (e->GetBlendEdgeuse() == nullptr) {
      skipped.push_back(i);
      continue;
    }
    kept.push_back(e);
    sel_ids.push_back(i);
  }
  if (kept.empty())
    throw fail(SC_FAILED,
               std::string(what) + ": none of the edges can be blended (smooth, seam or open edges)",
               skipped);

  BrepPtr b;
  try {
    b = blend_core(in, kept, sel_ids, r, chamfer, laws);
  } catch (const Failure& whole) {
    auto chains = laws ? std::vector<std::vector<size_t>>() : edge_chains(kept);
    if (chains.size() < 2) throw;
    double tol = 1e-7 * std::max(1.0, model_diag(in));
    BrepPtr cur = copy_brep(in);
    for (const auto& ch : chains) {
      std::vector<SmEdge*> es;
      std::vector<uint32_t> os;
      for (size_t k : ch) {
        SmEdge* e = matching_edge(cur.get(), kept[k], tol);
        if (!e) throw whole;  // an earlier chain changed this one: they are not separate
        es.push_back(e);
        os.push_back(sel_ids[k]);
      }
      g_trail.clear();
      cur = blend_core(cur.get(), es, os, r, chamfer, nullptr);
    }
    b = std::move(cur);
  }
  size_t fresh = resolve_tags(b.get(), {in}, tag, true);
  if (fresh == 0)
    throw fail(SC_FAILED,
               std::string(what) + " failed: SMLib returned the shape unchanged (size " +
                   std::to_string(r) + " too large?)",
               sel_ids);
  return put(std::move(b));
}

// ------------------------------------------------------------------ validated booleans
//
// SMLib's two-Brep Booleans occasionally return SM_SUCCESS with a closed, valid-looking solid
// that is geometrically wrong: whole face regions of an operand are classified to the wrong
// side and dropped. Reproducer: three orthogonal cylinders of equal radius (r = 4, h = 30,
// centred); (Z u Y) u X loses the -x arm of the X cylinder (volume 3237 instead of 3800,
// bounds x >= -4), (Z u X) u Y is correct, (Y u X) u Z fails with SM_ERR. Only equal radii
// fail (3.9 or 4.1 for the third cylinder work): the untrimmed cylinder pairs then intersect
// in two ellipses that cross at tangent points, a singular case for the surface intersection.
// The facade therefore checks every pairwise result (see boolean_plausible) and on a mismatch
// retries with swapped operands, a different order of the remaining operands (sc_boolean),
// and finally with the second operand shifted by 1e-6..1e-4 of the model size (far below
// every export tolerance). If nothing passes the check, SMLib's own result is kept, as
// before (see checked_boolean).

struct Box3 {
  V3 mn = v3(1e300, 1e300, 1e300), mx = v3(-1e300, -1e300, -1e300);
  bool empty() const { return mn.x > mx.x; }
  void add(const Box3& b) {
    if (b.empty()) return;
    mn = v3(std::min(mn.x, b.mn.x), std::min(mn.y, b.mn.y), std::min(mn.z, b.mn.z));
    mx = v3(std::max(mx.x, b.mx.x), std::max(mx.y, b.mx.y), std::max(mx.z, b.mx.z));
  }
  double diag() const { return empty() ? 0.0 : len(mx - mn); }
  bool overlaps(const Box3& b, double tol) const {
    return !empty() && !b.empty() && mn.x <= b.mx.x + tol && b.mn.x <= mx.x + tol &&
           mn.y <= b.mx.y + tol && b.mn.y <= mx.y + tol && mn.z <= b.mx.z + tol &&
           b.mn.z <= mx.z + tol;
  }
};

// Loose box (union of the vertex, edge and face boxes; cheap). The tight box costs more than
// the Boolean itself on bodies with many faces.
Box3 loose_box(const SmBrep* b) {
  Box3 r;
  if (!b || b->GetNumFaces() == 0) return r;
  SmPoint3d mn, mx;
  if (SmApiBrepBoundingBox(b, FALSE, mn, mx) != SM_SUCCESS) {
    r.mn = v3(-1e300, -1e300, -1e300);  // unknown: never constrains
    r.mx = v3(1e300, 1e300, 1e300);
    return r;
  }
  r.mn = fromsm(mn);
  r.mx = fromsm(mx);
  return r;
}

Box3 face_box(const SmFace* f) {
  Box3 r;
  SmPoint3d mn, mx;
  if (SmApiFaceBoundingBox(f, FALSE, mn, mx) == SM_SUCCESS) {
    r.mn = fromsm(mn);
    r.mx = fromsm(mx);
  } else {
    r.mn = v3(-1e300, -1e300, -1e300);
    r.mx = v3(1e300, 1e300, 1e300);
  }
  return r;
}

// Coarse volume (relative accuracy 1e-2: much cheaper than 1e-6, enough for the inequalities).
double solid_volume(const SmBrep* b) {
  if (!b || b->GetNumFaces() == 0) return 0.0;
  double v = 0;
  if (SmApiBrepComputeVolume(b, 1e-2, v) != SM_SUCCESS || !is_fin(v)) return NAN;
  return v;
}

// Up to 5 points strictly inside face `f` with the outward normal of the solid at each. Fixed
// off-centre UV fractions (the UV centre of a revolved face lies in a symmetry plane, where
// SMLib's ray-based point classification is least reliable); the face's internal point only
// if none of them lies on the face.
void face_samples(const SmFace* f, std::vector<std::pair<V3, V3>>& out) {
  SmSurface* srf = f->GetSurface();
  if (!srf) return;
  SmExtent2d d = f->GetUVDomain();
  const double st[5][2] = {{0.27, 0.23}, {0.73, 0.31}, {0.21, 0.77}, {0.69, 0.71}, {0.53, 0.46}};
  bool flip = false;
  {
    SmFaceuse* fu = f->GetUpwardFaceuse();
    flip = fu && fu->GetShell() && fu->GetShell()->GetRegion() &&
           !fu->GetShell()->GetRegion()->IsVoid();
  }
  size_t before = out.size();
  for (const auto& s : st) {
    SmPoint2d uv(d.GetUMin() + s[0] * (d.GetUMax() - d.GetUMin()),
                 d.GetVMin() + s[1] * (d.GetVMax() - d.GetVMin()));
    SmPointClassification pc;
    if (f->PointClassify(uv, SmTol::GetZoneTol3d(f), FALSE, TRUE, pc) != SM_SUCCESS ||
        pc.GetPointClass() != SM_PC_FACE)
      continue;
    SmVector2d uvv(uv.x, uv.y);
    SmVector3d pv, nv;
    if (SmApiEvaluateSurfacePoint(srf, uvv, pv) != SM_SUCCESS) continue;
    if (SmApiEvaluateSurfaceNormal(srf, uvv, nv) != SM_SUCCESS) continue;
    V3 nn = normalize(v3(nv.x, nv.y, nv.z));
    if (len(nn) == 0) continue;
    out.push_back({v3(pv.x, pv.y, pv.z), flip ? nn * -1.0 : nn});
  }
  V3 p, n;
  if (out.size() == before && face_sample(f, p, n)) out.push_back({p, n});
}

// Expected membership of point q in (A op B), from A's and B's classification;
// -1 = undecidable (q on a boundary or classification failed).
int expected_class(int32_t op, int ca, int cb) {
  if (ca < 0 || cb < 0 || ca == 2 || cb == 2) return -1;
  bool a = ca == 1, b = cb == 1;
  bool in = op == SC_UNION ? (a || b) : op == SC_DIFFERENCE ? (a && !b) : (a && b);
  return in ? 1 : 0;
}

// Is R a plausible result of A op B? `why` gets the first violated condition.
//  1. volume inequalities of the operation (coarse volumes, 3 % slack: gross errors only);
//  2. membership of points just inside and just outside the faces of each operand that reach
//     the other operand's box (5 samples per face): a point inside A, outside B must be
//     inside A u B and A - B, and so on. A mismatch counts only if it persists at a second,
//     shifted point with all three bodies classified again (SMLib's ray classification
//     can be wrong at single points).
// PL_OPEN: the result of two closed solids is not closed. That is legitimate for bodies
// touching along an edge, but also a symptom of the misclassification (box minus the three
// cylinders above, in one order). Operands whose boxes do not overlap are not checked.
enum Plausible { PL_OK, PL_OPEN, PL_WRONG };

// `va_known`: volume of A if already known (NAN: compute); `vr_out` gets R's volume when it
// was computed (NAN otherwise), so a chain of Booleans computes each volume once.
Plausible boolean_plausible(int32_t op, const SmBrep* A, const SmBrep* B, const SmBrep* R,
                            std::string& why, double va_known = NAN, double* vr_out = nullptr) {
  if (vr_out) *vr_out = NAN;
  bool ra = A->GetNumFaces() > 0 && A->IsManifoldSolid();
  bool rb = B->GetNumFaces() > 0 && B->IsManifoldSolid();
  if (!ra || !rb) return PL_OK;  // only closed solids can be checked
  bool r_empty = !R || R->GetNumFaces() == 0;
  if (!r_empty && !R->IsManifoldSolid()) {
    why = "result is not a closed solid";
    return PL_OPEN;
  }
  Box3 ba = loose_box(A), bb = loose_box(B);
  Box3 all = ba;
  all.add(bb);
  double size = std::max(all.diag(), 1e-9);
  double eps = 1e-4 * size;
  if (!ba.overlaps(bb, eps)) return PL_OK;

  double va = is_fin(va_known) ? va_known : solid_volume(A), vb = solid_volume(B),
         vr = r_empty ? 0.0 : solid_volume(R);
  if (vr_out) *vr_out = vr;
  if (is_fin(va) && is_fin(vb) && is_fin(vr)) {
    double s = 3e-2 * (va + vb) + 1e-12;
    bool ok = true;
    if (op == SC_UNION) ok = vr >= std::max(va, vb) - s && vr <= va + vb + s;
    else if (op == SC_DIFFERENCE) ok = vr >= va - vb - s && vr <= va + s;
    else ok = vr >= -s && vr <= std::min(va, vb) + s;
    if (!ok) {
      why = "volume " + std::to_string(vr) + " impossible for operands " + std::to_string(va) +
            ", " + std::to_string(vb);
      return PL_WRONG;
    }
  }

  auto check_point = [&](V3 q, int ca, int cb) -> int {  // 1 = consistent, 0 = not, -1 = n/a
    int want = expected_class(op, ca, cb);
    if (want < 0) return -1;
    int got = r_empty ? 0 : classify_point(R, q);
    if (got < 0 || got == 2) return -1;
    return got == want ? 1 : 0;
  };
  for (int side = 0; side < 2; side++) {
    const SmBrep* self = side == 0 ? A : B;
    const SmBrep* other = side == 0 ? B : A;
    const Box3& obox = side == 0 ? bb : ba;
    std::vector<std::pair<V3, V3>> samples;
    for (SmFace* f : faces_of(self))
      if (face_box(f).overlaps(obox, eps)) face_samples(f, samples);
    for (const auto& smp : samples) {
      for (double sign : {-1.0, 1.0}) {
        V3 q = smp.first + smp.second * (sign * eps);
        int cs = sign < 0 ? 1 : 0;  // just inside / outside the own operand
        int co = classify_point(other, q);
        int ok = check_point(q, side == 0 ? cs : co, side == 0 ? co : cs);
        if (ok != 0) continue;
        // confirm at a second point, everything classified again
        V3 t = normalize(cross(smp.second, v3(0.48, 0.6, 0.64)));
        if (len(t) == 0) t = normalize(cross(smp.second, v3(1, 0, 0)));
        V3 q2 = q + t * (0.37 * eps);
        if (check_point(q2, classify_point(A, q2), classify_point(B, q2)) != 0) continue;
        int got = classify_point(R, q);
        why = "point (" + std::to_string(q.x) + ", " + std::to_string(q.y) + ", " +
              std::to_string(q.z) + ") is " + (got == 1 ? "inside" : "outside") +
              " the result but must be " + (got == 1 ? "outside" : "inside");
        return PL_WRONG;
      }
    }
  }
  return PL_OK;
}

// One SMLib Boolean on copies of A and B. Returns the result (an empty Brep for an empty
// result) or nullptr on an SMLib error (`err` gets the message).
BrepPtr raw_boolean(int32_t op, const SmBrep* A, const SmBrep* B, std::string& err) {
  BrepPtr a = copy_brep(A);
  BrepPtr b = copy_brep(B);
  SmBrep* res = nullptr;
  SmStatus st;
  const char* what;
  g_trail.clear();
  if (op == SC_UNION) {
    st = SmApiBooleanUnion(a.get(), b.get(), res);
    what = "boolean union";
  } else if (op == SC_DIFFERENCE) {
    st = SmApiBooleanDifference(a.get(), b.get(), res);
    what = "boolean difference";
  } else {
    st = SmApiBooleanIntersection(a.get(), b.get(), res);
    what = "boolean intersection";
  }
  if (st != SM_SUCCESS) {
    err = sm_fail(what, st).msg;
    return BrepPtr();
  }
  b.release();  // deleted by SMLib on success
  if (res != a.get()) {
    a.release();
    a.reset(res);
  }
  if (!a.get()) a = empty_brep();
  return a;
}

// Do the interiors of the closed solids A and B overlap? Points just inside the faces of each
// that reach the other's box must not be inside the other, and A n B must have no volume.
// SMLib errors count as overlap (the caller then keeps its previous behaviour).
bool interiors_overlap(const SmBrep* A, const SmBrep* B) {
  Box3 ba = loose_box(A), bb = loose_box(B);
  Box3 all = ba;
  all.add(bb);
  double eps = 1e-4 * std::max(all.diag(), 1e-9);
  for (int side = 0; side < 2; side++) {
    const SmBrep* self = side == 0 ? A : B;
    const SmBrep* other = side == 0 ? B : A;
    const Box3& obox = side == 0 ? bb : ba;
    std::vector<std::pair<V3, V3>> samples;
    for (SmFace* f : faces_of(self))
      if (face_box(f).overlaps(obox, eps)) face_samples(f, samples);
    for (const auto& smp : samples)
      if (classify_point(other, smp.first - smp.second * eps) == 1) return true;
  }
  std::string e;
  BrepPtr x = raw_boolean(SC_INTERSECTION, A, B, e);
  if (!x.get()) return true;
  if (x->GetNumFaces() == 0) return false;
  double vx = solid_volume(x.get());
  double vmin = std::min(solid_volume(A), solid_volume(B));
  return !(is_fin(vx) && is_fin(vmin) && std::fabs(vx) <= 1e-6 * vmin);
}

// Union of closed solids that only touch (along an edge, at points): SMLib returns a
// non-manifold body there, which is not a closed solid (volume 0, winkel_anker). Instead both
// bodies go into one Brep as separate shells (SmBrep::MergeBrep, no intersection), which is a
// closed solid with the right volume. nullptr if the interiors overlap or the merge fails.
BrepPtr touching_union(const SmBrep* A, const SmBrep* B) {
  if (interiors_overlap(A, B)) return BrepPtr();
  BrepPtr m = copy_brep(A);
  if (m->MergeBrep(*B, nullptr, nullptr, FALSE) != SM_SUCCESS) return BrepPtr();
  if (m->GetNumFaces() == 0 || !m->IsManifoldSolid()) return BrepPtr();
  return m;
}

// A op B with the plausibility check and fallbacks (see above). `allow_perturb` = false
// leaves out the shifted retries (used while other operand orders are still untried); they
// are only tried after a wrong result or an SMLib error, not for results that are merely
// open. Returns nullptr if nothing passes; `plain` then holds SMLib's own result for A op B
// (if it produced one), which the caller takes as a last resort: the check itself relies on
// SMLib's point classification, which errs on some bodies (revolved splines), so a result
// it rejects is not necessarily wrong, and it is what the facade returned before the check.
// Not so (`plain_impossible`) when its volume is impossible for the operation (volumes do not
// depend on the point classification), or when it is not a closed solid: operands that touch
// tangentially along a curve while their interiors overlap (cylinder minus a torus touching
// its top and bottom faces; a dome touching a wall) have no manifold result, SMLib returns a
// non-manifold body that measures volume 0 and fails later operations. Operands that only
// touch are handled before (touching_union; a - b = a, a n b = empty).
// `va`: volume of A if known (NAN otherwise); on success it is set to the result's volume
// (NAN if not computed).
BrepPtr checked_boolean(int32_t op, const SmBrep* A, const SmBrep* B, bool allow_perturb,
                        std::string& err, BrepPtr& plain, double& va,
                        bool* plain_impossible = nullptr) {
  bool commutative = op != SC_DIFFERENCE;
  bool wrong = false;
  std::string why;
  // runs a op b (swapped: b op a) and checks the result against a op b
  bool first = true;
  auto attempt = [&](const SmBrep* a, const SmBrep* b, bool swapped) -> BrepPtr {
    std::string e;
    BrepPtr r = swapped ? raw_boolean(op, b, a, e) : raw_boolean(op, a, b, e);
    bool is_plain = first;
    first = false;
    if (!r.get()) {
      wrong = true;
      if (why.empty()) why = e;
      return BrepPtr();
    }
    std::string w;
    double vr = NAN;
    Plausible pl = boolean_plausible(op, a, b, r.get(), w, a == A ? va : NAN, &vr);
    if (pl == PL_OK) {
      va = vr;
      return r;
    }
    if (pl == PL_OPEN && op == SC_UNION) {
      BrepPtr t = touching_union(a, b);
      if (t.get()) {
        va = NAN;
        return t;
      }
    }
    // Bodies that only touch: a - b = a, a n b = empty (SMLib returns a non-manifold body).
    if (pl == PL_OPEN && op != SC_UNION && !interiors_overlap(a, b)) {
      va = NAN;
      if (op == SC_DIFFERENCE) return swapped ? copy_brep(b) : copy_brep(a);
      return empty_brep();
    }
    if (pl == PL_WRONG) wrong = true;
    if (is_plain) {
      plain = std::move(r);
      if (plain_impossible)
        *plain_impossible =
            (pl == PL_WRONG && w.rfind("volume ", 0) == 0) || pl == PL_OPEN;
    }
    if (why.empty())
      why = pl == PL_OPEN ? "the result is not a closed solid: the operands touch tangentially or "
                            "along an edge there (let them overlap a little or keep a gap)"
                          : "SMLib returned a wrong result (" + w + ")";
    return BrepPtr();
  };
  BrepPtr r = attempt(A, B, false);
  if (r.get()) return r;
  if (commutative) {
    r = attempt(A, B, true);
    if (r.get()) return r;
  }
  if (allow_perturb && wrong) {
    Box3 all = loose_box(A);
    all.add(loose_box(B));
    double size = std::max(all.diag(), 1e-9);
    V3 dir = normalize(v3(0.2672612, 0.5345225, 0.8017837));
    for (double rel : {1e-6, 1e-5, 1e-4}) {
      BrepPtr bs = copy_brep(B);
      M4 t = identity();
      V3 off = dir * (rel * size);
      t.m[12] = off.x;
      t.m[13] = off.y;
      t.m[14] = off.z;
      apply_matrix(bs.get(), t);
      r = attempt(A, bs.get(), false);
      if (r.get()) return r;
      if (commutative) {
        r = attempt(A, bs.get(), true);
        if (r.get()) return r;
      }
    }
  }
  err = why;
  return BrepPtr();
}

// ------------------------------------------------------------------ shell pre-check

// A wall that does not fit leaves no cavity. usd-brep c9c5979 read freed memory there
// (valgrind: invalid reads in SmProtoEdge::RemoveProtoVertex for a 10 mm cube already at
// t = 4.99; solidcore plan/upstream-issues.md UB-175, UB-239, UB-242) and then failed,
// returned a solid or crashed. With the fork's fixes (smlib/fetch.sh) valgrind is clean for
// cubes (closed and open), spheres and cylinders up to t = 2 r, but the closed cube with
// t >= 5 comes back as an empty Brep with SM_SUCCESS; open bodies, spheres and cylinders fail
// with SM_ERR. So this check is no longer a memory-safety guard; it stays as the fast path
// with a clear message for the common case (two antiparallel planar walls, not opened,
// facing each other at a distance below `limit` = 2 t), and sc_shell rejects empty or open
// results after SMLib for everything else.
bool opposite_walls_closer_than(const SmBrep* b, const std::vector<const SmFace*>& open,
                                double limit, double& gap_out) {
  struct Wall {
    SmFace* f;
    V3 p, n;  // a point and the outward normal
    std::vector<std::pair<V3, V3>> samples;
  };
  std::vector<Wall> walls;
  for (SmFace* f : faces_of(b)) {
    if (std::find(open.begin(), open.end(), f) != open.end()) continue;
    const SmSurface* srf = f->GetSurface();
    if (!srf || !srf->IsKindOf(SmPlane_TYPE)) continue;
    Wall w{f, v3(0, 0, 0), v3(0, 0, 0), {}};
    face_samples(f, w.samples);
    if (w.samples.empty()) continue;
    w.p = w.samples[0].first;
    w.n = w.samples[0].second;
    walls.push_back(std::move(w));
  }
  double tol = 1e-9 * std::max(1.0, model_diag(b));
  for (const Wall& a : walls)
    for (const Wall& c : walls) {
      if (&a == &c || dot(a.n, c.n) > -0.999) continue;
      for (const auto& smp : a.samples) {
        double gap = -dot(smp.first - c.p, c.n);  // distance to c's plane, > 0 on its inner side
        if (!(gap > tol) || gap > limit * (1 + 1e-9)) continue;  // no cavity left at gap = 2 t
        // the foot point must lie on face c (walls really face each other there)
        V3 foot = smp.first - c.n * (-gap);
        if (face_distance(c.f, foot) > 1e-6 * std::max(1.0, gap)) continue;
        gap_out = gap;
        return true;
      }
    }
  return false;
}

// ------------------------------------------------------------------ shell from booleans
//
// SmApiShellBrepFull with opened faces fails on rounded bodies (box with $fillet: SM_ERR,
// SM_ERR_NULL_POINTER in SmMerge, or a solid whose rim faces are missing and whose round
// offsets cross each other: about a dozen faces fail invalid_faces, the volume is still close),
// although the closed inward offset of the same body is exact. sc_shell then builds the shell
// from Booleans instead:
//
//   result = body - O - P_1 - ... - P_k
//
// O is the closed inward offset of the body (SmApiOffsetBrepFull, -t), P_i a prism over the
// face of O that is the offset of the i-th opened face: from just inside O's cavity along the
// face's outward normal to past the body. Its side walls run through the boundary of that
// offset face, so the opening's rim is the ruled band between the opened face and its offset,
// as in SMLib's own shell. Only planar opened faces are handled (the prism is the thickened
// offset face); otherwise the fallback gives up and sc_shell reports SMLib's error.

// Index of the face of `o` that lies on the inward offset (by `t`) of the planar face `f`.
long offset_face_index(const SmBrep* o, const SmFace* f, double t, double tol) {
  V3 p, n;
  if (!face_sample(f, p, n)) return -1;
  V3 q = p - n * t;
  auto fs = faces_of(o);
  long best = -1;
  double bd = 1e300;
  for (size_t i = 0; i < fs.size(); i++) {
    const SmSurface* s = fs[i]->GetSurface();
    if (!s || !s->IsKindOf(SmPlane_TYPE)) continue;
    V3 pp, nn;
    if (!face_sample(fs[i], pp, nn) || dot(nn, n) < 1 - 1e-9) continue;
    if (std::fabs(dot(pp - q, n)) > tol) continue;
    double d = face_distance(fs[i], q);
    if (d < bd) {
      bd = d;
      best = (long)i;
    }
  }
  return bd <= tol ? best : -1;
}

// A - B with the plausibility check; SMLib's own result if the check rejects every attempt.
BrepPtr checked_difference(const SmBrep* A, const SmBrep* B, std::string& err) {
  BrepPtr plain;
  double va = NAN;
  BrepPtr r = checked_boolean(SC_DIFFERENCE, A, B, true, err, plain, va);
  if (!r.get()) r = std::move(plain);
  return r;
}

BrepPtr shell_by_booleans(const SmBrep* in, double t, const std::vector<const SmFace*>& open,
                          std::string& why) {
  for (const SmFace* f : open) {
    const SmSurface* s = f->GetSurface();
    if (!s || !s->IsKindOf(SmPlane_TYPE)) {
      why = "opened face is not planar";
      return BrepPtr();
    }
  }
  double vin = solid_volume(in);
  double tol = 1e-6 * std::max(1.0, model_diag(in));
  // 1. the inward offset, a closed solid with a cavity's volume
  BrepPtr O;
  for (int k = 0; k < 2 && !O.get(); k++) {
    BrepPtr c = copy_brep(in);
    SmBrep* o = nullptr;
    g_trail.clear();
    SmStatus st = SmApiOffsetBrepFull(c.get(), -t, k == 1, k == 1, o);
    BrepPtr ob(o);
    if (st != SM_SUCCESS || !o) {
      why = sm_fail("SmApiOffsetBrepFull", st == SM_SUCCESS ? SM_ERR : st).msg;
      continue;
    }
    if (o->GetNumFaces() == 0 || !o->IsManifoldSolid()) {
      why = "the inward offset is no closed solid (no cavity left)";
      continue;
    }
    double vo = solid_volume(o);
    if (!(vo > 0) || !(vo < vin)) {
      why = "the inward offset has an impossible volume";
      continue;
    }
    if (!invalid_faces(o).empty()) {
      why = "the inward offset overlaps itself";
      continue;
    }
    O = std::move(ob);
  }
  if (!O.get()) return BrepPtr();
  // 2. prisms over the offsets of the opened faces
  std::vector<BrepPtr> prisms;
  double delta = 0.5 * t, margin = t;
  for (const SmFace* f : open) {
    long idx = offset_face_index(O.get(), f, t, 1e3 * tol);
    if (idx < 0) {
      why = "no offset of an opened face found";
      return BrepPtr();
    }
    V3 p, n;
    face_sample(f, p, n);
    BrepPtr sheet = copy_brep(O.get());
    {
      auto sf = faces_of(sheet.get());
      SmTArray<SmFace*> rm;
      for (size_t i = 0; i < sf.size(); i++)
        if ((long)i != idx) rm.Add(sf[i]);
      SC_CHECK(sheet->RemoveFaces(rm), "SmBrep::RemoveFaces");
      if (sheet->GetNumFaces() != 1) {
        why = "offset face could not be isolated";
        return BrepPtr();
      }
    }
    M4 m = identity();
    V3 d = n * -delta;
    m.m[12] = d.x;
    m.m[13] = d.y;
    m.m[14] = d.z;
    apply_matrix(sheet.get(), m);
    BrepPtr prism;
    for (double sgn : {1.0, -1.0}) {
      SmTArray<const SmFace*> none;
      SmBrep* pr = nullptr;
      SmStatus st = SmApiShellBrepFull(sheet.get(), sgn * (t + delta + margin), TRUE, TRUE, TRUE, none, pr);
      BrepPtr pb(pr);
      if (st != SM_SUCCESS || !pr || pr->GetNumFaces() == 0 || !pr->IsManifoldSolid()) continue;
      // the prism must reach past the opened face
      if (classify_point(pr, p + n * (0.5 * margin)) != 1) continue;
      prism = std::move(pb);
      break;
    }
    if (!prism.get()) {
      why = "prism over an opened face failed";
      return BrepPtr();
    }
    prisms.push_back(std::move(prism));
  }
  // 3. body - O - prisms
  std::string err;
  BrepPtr r = checked_difference(in, O.get(), err);
  for (size_t i = 0; r.get() && i < prisms.size(); i++) r = checked_difference(r.get(), prisms[i].get(), err);
  if (!r.get()) {
    why = err.empty() ? "boolean failed" : err;
    return BrepPtr();
  }
  if (r->GetNumFaces() == 0 || !r->IsManifoldSolid()) {
    why = "result is not a closed solid";
    return BrepPtr();
  }
  if (!invalid_faces(r.get()).empty()) {
    why = "result overlaps itself";
    return BrepPtr();
  }
  return r;
}

// ------------------------------------------------------------------ tessellation

// Free-form faces: not planar, no surface of revolution/extrusion, sphere or torus.
bool free_form(const SmFace* f) {
  const SmSurface* s = f->GetSurface();
  if (!s) return false;
  return !(s->IsKindOf(SmPlane_TYPE) || s->IsKindOf(SmSurfOfRevolution_TYPE) ||
           s->IsKindOf(SmSurfOfExtrusion_TYPE) || s->IsKindOf(SmSphere_TYPE) ||
           s->IsKindOf(SmTorus_TYPE));
}

// Number of polygons and their total area.
size_t poly_triangles(SmPolyBrep* poly, double* area) {
  SmTArray<SmPolyFace*> pfs;
  poly->GetPolyFaces(pfs);
  double a = 0;
  for (ULONG i = 0; i < pfs.GetSize(); i++) {
    SmTArray<SmPolyEdge*> pe;
    pfs[i]->GetPolyEdges(pe);
    if (pe.GetSize() < 3) continue;
    V3 p0 = fromsm(pe[0]->GetStartPolyVertex()->GetPoint());
    for (ULONG j = 1; j + 1 < pe.GetSize(); j++) {
      V3 p1 = fromsm(pe[j]->GetStartPolyVertex()->GetPoint());
      V3 p2 = fromsm(pe[j + 1]->GetStartPolyVertex()->GetPoint());
      a += 0.5 * len(cross(p1 - p0, p2 - p0));
    }
  }
  if (area) *area = a;
  return pfs.GetSize();
}

// Largest distance between a triangle centroid and its face's surface over the free-form
// faces of `poly` (up to 64 triangles per face), and the longest edge of that triangle.
void tess_deviation(SmPolyBrep* poly, const std::unordered_map<const SmFace*, uint32_t>& face_ix,
                    double& worst, double& worst_edge) {
  worst = 0;
  worst_edge = 0;
  std::unordered_map<const SmFace*, int> ff;  // face -> free-form?
  std::unordered_map<const SmFace*, uint32_t> seen;
  SmTArray<SmPolyFace*> pfs;
  poly->GetPolyFaces(pfs);
  for (ULONG i = 0; i < pfs.GetSize(); i++) {
    SmFace* of = pfs[i]->GetOriginalFace();
    if (!of || face_ix.find(of) == face_ix.end()) continue;
    auto it = ff.find(of);
    if (it == ff.end()) it = ff.emplace(of, free_form(of) ? 1 : 0).first;
    if (!it->second) continue;
    if (seen[of]++ >= 64) continue;
    SmTArray<SmPolyEdge*> pe;
    pfs[i]->GetPolyEdges(pe);
    if (pe.GetSize() < 3) continue;
    V3 c = v3(0, 0, 0);
    double emax = 0;
    for (ULONG j = 0; j < pe.GetSize(); j++) {
      V3 a = fromsm(pe[j]->GetStartPolyVertex()->GetPoint());
      V3 z = fromsm(pe[(j + 1) % pe.GetSize()]->GetStartPolyVertex()->GetPoint());
      c = c + a;
      emax = std::max(emax, len(z - a));
    }
    c = c * (1.0 / pe.GetSize());
    double dvert = 1e300;  // the vertices lie on the surface: an upper bound of the distance
    for (ULONG j = 0; j < pe.GetSize(); j++)
      dvert = std::min(dvert, len(fromsm(pe[j]->GetStartPolyVertex()->GetPoint()) - c));
    SmPoint3d cp;
    SmTArray<SmPoint2d> uvs;
    double d = 0;
    if (SmApiSurfaceClosestPoint(of->GetSurface(), sp(c), cp, uvs, d) != SM_SUCCESS || !is_fin(d))
      continue;
    // a "closest" point farther than a vertex is a failed solve (seen on revolved splines
    // near the axis): ignore it
    if (d >= 0.5 * dvert) continue;
    if (d > worst) {
      worst = d;
      worst_edge = emax;
    }
  }
}

struct PairHash {
  size_t operator()(const std::pair<const void*, uint32_t>& p) const {
    return std::hash<const void*>()(p.first) * 31u + p.second;
  }
};


// Edge polylines of a tessellation. Mesh edges between triangles of different faces are
// assigned to the Brep edge between those faces (nearest one if several), so the lines
// coincide with the mesh. Edges without such mesh edges (seams, degenerate cases) are
// sampled with SmApiTessellateBoundaries.
void emit_edge_segments(SmBrep* b, const std::vector<SmFace*>& fs, const std::vector<SmEdge*>& es,
                        const std::unordered_map<const SmFace*, uint32_t>& face_ix,
                        const std::unordered_map<const SmEdge*, uint32_t>& edge_ix,
                        const std::map<std::pair<const void*, const void*>, std::vector<uint32_t>>& medge,
                        const std::unordered_map<const void*, V3>& pvpos, std::vector<float>& seg,
                        std::vector<uint32_t>& eid) {
  (void)fs;
  std::vector<std::vector<uint32_t>> eface(es.size());
  std::vector<std::vector<V3>> esamp(es.size());
  for (size_t i = 0; i < es.size(); i++) {
    SmTArray<SmFace*> ef;
    es[i]->GetFaces(ef);
    for (ULONG k = 0; k < ef.GetSize(); k++) {
      auto it = face_ix.find(ef[k]);
      if (it != face_ix.end()) eface[i].push_back(it->second);
    }
    std::sort(eface[i].begin(), eface[i].end());
    eface[i].erase(std::unique(eface[i].begin(), eface[i].end()), eface[i].end());
    SmCurve* c = es[i]->GetCurve();
    SmExtent1d iv = es[i]->GetInterval();
    for (int k = 0; c && k <= 32; k++) {
      SmPoint3d p;
      if (c->EvaluatePoint(iv.GetMin() + (iv.GetMax() - iv.GetMin()) * k / 32.0, p) == SM_SUCCESS)
        esamp[i].push_back(fromsm(p));
    }
  }
  auto dist_to = [&](size_t e, V3 p) {
    double best = 1e300;
    const auto& s = esamp[e];
    for (size_t k = 0; k + 1 < s.size(); k++) {
      V3 a = s[k], ab = s[k + 1] - s[k];
      double l2 = dot(ab, ab);
      double t = l2 > 0 ? std::min(1.0, std::max(0.0, dot(p - a, ab) / l2)) : 0.0;
      best = std::min(best, len(a + ab * t - p));
    }
    if (s.size() == 1) best = len(s[0] - p);
    return best;
  };
  std::vector<bool> has(es.size(), false);
  for (const auto& kv : medge) {
    std::vector<uint32_t> u = kv.second;
    size_t uses = u.size();
    std::sort(u.begin(), u.end());
    u.erase(std::unique(u.begin(), u.end()), u.end());
    if (u.size() == 1 && uses >= 2) continue;  // interior mesh edge
    std::vector<size_t> cand;
    for (size_t e = 0; e < es.size(); e++)
      if (eface[e] == u) cand.push_back(e);
    if (cand.empty())
      for (size_t e = 0; e < es.size(); e++)
        for (uint32_t f : u)
          if (std::find(eface[e].begin(), eface[e].end(), f) != eface[e].end()) {
            cand.push_back(e);
            break;
          }
    if (cand.empty()) continue;
    auto pa = pvpos.find(kv.first.first);
    auto pb = pvpos.find(kv.first.second);
    if (pa == pvpos.end() || pb == pvpos.end()) continue;
    V3 a = pa->second, c = pb->second;
    size_t best = cand[0];
    if (cand.size() > 1) {
      V3 mid = (a + c) * 0.5;
      double bd = 1e300;
      for (size_t e : cand) {
        double d = dist_to(e, mid);
        if (d < bd) {
          bd = d;
          best = e;
        }
      }
    }
    for (V3 p : {a, c}) {
      seg.push_back((float)p.x);
      seg.push_back((float)p.y);
      seg.push_back((float)p.z);
    }
    eid.push_back((uint32_t)best);
    has[best] = true;
  }
  bool missing = std::find(has.begin(), has.end(), false) != has.end();
  if (!missing) return;
  SmTArray<SmPoint3d> pts;
  SmTArray<ULONG> counts;
  SmTArray<SmEdge*> bedges;
  SmApiTessellateBoundaries(b, 10.0, pts, counts, bedges);  // partial output is fine
  ULONG off = 0;
  for (ULONG i = 0; i < bedges.GetSize() && i < counts.GetSize(); i++) {
    ULONG n = counts[i];
    auto it = edge_ix.find(bedges[i]);
    if (it != edge_ix.end() && !has[it->second]) {
      for (ULONG k = 0; k + 1 < n; k++) {
        for (ULONG q : {off + k, off + k + 1}) {
          seg.push_back((float)pts[q].x);
          seg.push_back((float)pts[q].y);
          seg.push_back((float)pts[q].z);
        }
        eid.push_back(it->second);
      }
    }
    off += n;
  }
}

}  // namespace

// ------------------------------------------------------------------ B-rep export (sc_brep)
//
// Topology and exact geometry of a Brep as one f64 stream, for STEP export (format in
// sc_facade.h, "sc_brep"). Every surface and curve is written in its exact NURBS form (SMLib's
// analytic classes are B-spline subclasses); analytic surfaces additionally carry their
// canonical parameters. Surfaces and curves that are not B-splines (blends, offsets, curves on
// surfaces) are approximated to the Brep's tolerance and flagged.

namespace brep_out {

const double SC_BREP_FORMAT = 1;

// surface classes
const double SURF_BSPLINE = 0, SURF_PLANE = 1, SURF_CYLINDER = 2, SURF_CONE = 3, SURF_SPHERE = 4,
             SURF_TORUS = 5, SURF_REVOLUTION = 6, SURF_EXTRUSION = 7, SURF_OTHER = 8;
// curve classes
const double CRV_BSPLINE = 0, CRV_LINE = 1, CRV_CIRCLE = 2, CRV_ELLIPSE = 3, CRV_OTHER = 4;

void put3(std::vector<double>& o, double x, double y, double z) {
  o.push_back(x);
  o.push_back(y);
  o.push_back(z);
}

// degree, n_ctrl, n_knots, knots..., (x y z w) per control point (w = 1 if non-rational)
void put_nurbs_curve(std::vector<double>& o, SmBSplineCurve* c) {
  gw_CURVE* nurb = c->GetOrCreateGwNurbPointer();
  if (!nurb) throw fail(SC_FAILED, "B-spline curve without NURBS data");
  uint32_t nk = (uint32_t)nurb->knt->m + 1;
  uint32_t nc = (uint32_t)nurb->pol->n + 1;
  o.push_back((double)nurb->p);
  o.push_back((double)nc);
  o.push_back((double)nk);
  for (uint32_t i = 0; i < nk; i++) o.push_back((double)nurb->knt->U[i]);
  for (uint32_t i = 0; i < nc; i++) {
    NL_CPOINT q = nurb->pol->Pw[i];
    double w = q.w == NL_NOW ? 1.0 : (double)q.w;
    double s = q.w == NL_NOW ? 1.0 : 1.0 / w;
    put3(o, q.x * s, q.y * s, q.z * s);
    o.push_back(w);
  }
}

// du, dv, nu, nv, n_ku, ku..., n_kv, kv..., (x y z w) per control point, u-major
void put_nurbs_surface(std::vector<double>& o, SmBSplineSurface* s) {
  NL_SURFACE* n = s->GetOrCreateGwNurbPointer();
  if (!n) throw fail(SC_FAILED, "B-spline surface without NURBS data");
  uint32_t nku = (uint32_t)n->knu->m + 1, nkv = (uint32_t)n->knv->m + 1;
  uint32_t nu = (uint32_t)n->net->n + 1, nv = (uint32_t)n->net->m + 1;
  o.push_back((double)n->p);
  o.push_back((double)n->q);
  o.push_back((double)nu);
  o.push_back((double)nv);
  o.push_back((double)nku);
  for (uint32_t i = 0; i < nku; i++) o.push_back((double)n->knu->U[i]);
  o.push_back((double)nkv);
  for (uint32_t i = 0; i < nkv; i++) o.push_back((double)n->knv->U[i]);
  for (uint32_t i = 0; i < nu; i++)
    for (uint32_t j = 0; j < nv; j++) {
      NL_CPOINT q = n->net->Pw[i][j];
      double w = q.w == NL_NOW ? 1.0 : (double)q.w;
      double sc = q.w == NL_NOW ? 1.0 : 1.0 / w;
      put3(o, q.x * sc, q.y * sc, q.z * sc);
      o.push_back(w);
    }
}

double approx_tol(const SmBrep* b) {
  double t = SmTol::GetZoneTol3d(b);
  for (SmFace* f : faces_of(b)) t = std::max(t, (double)SmTol::GetZoneTol3d(f));
  for (SmEdge* e : edges_of(b)) t = std::max(t, (double)SmTol::GetZoneTol3d(e));
  if (!(t > 0) || !is_fin(t)) t = 1e-6;
  SmXSectTol3d xt = SM_ZONE_TO_XSECTTOL3D(t);
  return (double)SmTol::GetApproxTol3d(xt);
}

// Edge: start vertex, end vertex, curve class, t0, t1, approximated (0/1), NURBS curve.
void put_edge(std::vector<double>& o, SmEdge* e,
              const std::unordered_map<const SmVertex*, uint32_t>& vix, double atol) {
  SmVertex* a = e->GetStartVertex();
  SmVertex* z = e->GetEndVertex();
  auto ia = vix.find(a), iz = vix.find(z);
  if (ia == vix.end() || iz == vix.end()) throw fail(SC_FAILED, "edge without vertices");
  SmCurve* c = e->GetCurve();
  if (!c) throw fail(SC_FAILED, "edge without curve");
  SmExtent1d iv = e->GetInterval();
  double cls = CRV_OTHER;
  if (c->IsKindOf(SmCircle_TYPE))
    cls = CRV_CIRCLE;
  else if (c->IsKindOf(SmEllipse_TYPE))
    cls = CRV_ELLIPSE;
  else if (c->IsKindOf(SmLine_TYPE))
    cls = CRV_LINE;
  else if (c->IsKindOf(SmBSplineCurve_TYPE))
    cls = CRV_BSPLINE;
  o.push_back((double)ia->second);
  o.push_back((double)iz->second);
  o.push_back(cls);
  if (cls != CRV_OTHER) {
    o.push_back(iv.GetMin());
    o.push_back(iv.GetMax());
    o.push_back(0);
    put_nurbs_curve(o, (SmBSplineCurve*)c);
    return;
  }
  SmTArray<double> breaks;
  breaks.Add(iv.GetMin());
  breaks.Add(iv.GetMax());
  double achieved = 0;
  SmBSplineCurve* bs = nullptr;
  SmStatus st = c->ApproximateCurve(*c->GetContext(), breaks, atol, achieved, bs, FALSE);
  ObjPtr<SmBSplineCurve> own(bs);
  if (st != SM_SUCCESS || !bs)
    throw sm_fail("SmCurve::ApproximateCurve", st == SM_SUCCESS ? SM_ERR : st);
  SmExtent1d biv = bs->GetNaturalInterval();
  o.push_back(biv.GetMin());
  o.push_back(biv.GetMax());
  o.push_back(1);
  put_nurbs_curve(o, bs);
}

// Surface: class, 13 parameters (origin, z axis, x axis, p1..p4), approximated (0/1), NURBS
// surface; revolution and extrusion surfaces are followed by their generator curve.
void put_surface(std::vector<double>& o, SmSurface* s, double atol) {
  double cls = SURF_OTHER;
  double prm[13] = {0};
  SmAxis2Placement a;
  auto frame = [&](const SmAxis2Placement& f) {
    SmPoint3d p = f.GetOrigin();
    SmVector3d z = f.GetZAxis(), x = f.GetXAxis();
    double v[9] = {p.x, p.y, p.z, z.x, z.y, z.z, x.x, x.y, x.z};
    for (int i = 0; i < 9; i++) prm[i] = v[i];
  };
  if (s->IsKindOf(SmPlane_TYPE)) {
    if (((SmPlane*)s)->GetCanonical(a) == SM_SUCCESS) {
      cls = SURF_PLANE;
      frame(a);
    }
  } else if (s->IsKindOf(SmCylinder_TYPE)) {
    double r = 0;
    if (((SmCylinder*)s)->GetCanonical(a, r) == SM_SUCCESS) {
      cls = SURF_CYLINDER;
      frame(a);
      prm[9] = r;
    }
  } else if (s->IsKindOf(SmCone_TYPE)) {
    double r = 0, ang = 0;
    if (((SmCone*)s)->GetCanonical(a, r, ang) == SM_SUCCESS) {
      cls = SURF_CONE;
      frame(a);
      prm[9] = r;
      prm[10] = ang * M_PI / 180.0;
    }
  } else if (s->IsKindOf(SmSphere_TYPE)) {
    double r = 0;
    if (((SmSphere*)s)->GetCanonical(a, r) == SM_SUCCESS) {
      cls = SURF_SPHERE;
      frame(a);
      prm[9] = r;
    }
  } else if (s->IsKindOf(SmTorus_TYPE)) {
    double R = 0, r = 0;
    if (((SmTorus*)s)->GetCanonical(a, R, r) == SM_SUCCESS) {
      cls = SURF_TORUS;
      frame(a);
      prm[9] = R;
      prm[10] = r;
    }
  } else if (s->IsKindOf(SmSurfOfRevolution_TYPE)) {
    SmSurfOfRevolution* rv = (SmSurfOfRevolution*)s;
    if (rv->GetGenCurve()) {
      cls = SURF_REVOLUTION;
      frame(rv->GetPosition());
    }
  } else if (s->IsKindOf(SmSurfOfExtrusion_TYPE)) {
    SmSurfOfExtrusion* ex = (SmSurfOfExtrusion*)s;
    if (ex->GetGenCurve()) {
      cls = SURF_EXTRUSION;
      SmVector3d d = ex->GetExtrusionVector();
      prm[3] = d.x;
      prm[4] = d.y;
      prm[5] = d.z;
    }
  } else if (s->IsKindOf(SmBSplineSurface_TYPE)) {
    cls = SURF_BSPLINE;
  }
  o.push_back(cls);
  for (double d : prm) o.push_back(d);
  if (s->IsKindOf(SmBSplineSurface_TYPE)) {
    o.push_back(0);
    put_nurbs_surface(o, (SmBSplineSurface*)s);
  } else {
    double achieved = 0;
    SmBSplineSurface* bs = nullptr;
    SmStatus st = s->ApproximateSurface(*s->GetContext(), atol, achieved, 5, bs, FALSE);
    ObjPtr<SmBSplineSurface> own(bs);
    if (!bs) throw sm_fail("SmSurface::ApproximateSurface", st == SM_SUCCESS ? SM_ERR : st);
    o.push_back(1);
    put_nurbs_surface(o, bs);
  }
  if (cls == SURF_REVOLUTION) put_nurbs_curve(o, ((SmSurfOfRevolution*)s)->GetGenCurve());
  if (cls == SURF_EXTRUSION) put_nurbs_curve(o, ((SmSurfOfExtrusion*)s)->GetGenCurve());
}

struct Ctx {
  std::unordered_map<const SmVertex*, uint32_t> vix;
  std::unordered_map<const SmEdge*, uint32_t> eix;
  std::unordered_map<const SmFace*, uint32_t> fix;
  double atol = 1e-6;
};

// Face: face index, tag, surface, sample (ok, point, normal, u, v), loops of `fu`.
// The loops of `fu` run counter-clockwise about its normal (SMLib's CCW edgeuse order); the
// sample normal is the normal of `fu` (out of the material for faces of solid regions).
void put_face(std::vector<double>& o, Ctx& cx, SmFaceuse* fu) {
  SmFace* f = fu->GetFace();
  uint32_t tag = 0;
  face_tag(f, tag);
  o.push_back((double)cx.fix.at(f));
  o.push_back((double)tag);
  put_surface(o, f->GetSurface(), cx.atol);
  V3 p = v3(0, 0, 0), n = v3(0, 0, 0);
  SmPoint2d uv(0, 0);
  bool ok = face_sample(f, p, n, &uv);
  if (ok) {
    // normal of the faceuse whose loops are written (face_sample orients it out of solids)
    SmVector3d nv;
    if (SmApiEvaluateSurfaceNormal(f->GetSurface(), SmVector2d(uv.x, uv.y), nv) == SM_SUCCESS) {
      n = normalize(v3(nv.x, nv.y, nv.z));
      if (fu->GetOrientation() != SM_OT_SAME) n = n * -1.0;
    } else {
      ok = false;
    }
  }
  o.push_back(ok ? 1 : 0);
  put3(o, p.x, p.y, p.z);
  put3(o, n.x, n.y, n.z);
  o.push_back(uv.x);
  o.push_back(uv.y);
  o.push_back(fu->GetOrientation() == SM_OT_SAME ? 1 : 0);
  SmTArray<SmLoopuse*> lus;
  fu->GetLoopuses(lus);
  o.push_back((double)lus.GetSize());
  for (ULONG i = 0; i < lus.GetSize(); i++) {
    SmLoopuse* lu = lus[i];
    if (lu->IsVertexLoopuse()) {
      SmTArray<SmEdgeuse*> eus;
      SmTArray<SmVertexuse*> vus;
      lu->GetEdgeusesAndVertexuses(eus, vus);
      if (vus.GetSize() == 0 || !vus[0]->GetVertex())
        throw fail(SC_FAILED, "vertex loop without vertex");
      o.push_back(0);
      o.push_back((double)cx.vix.at(vus[0]->GetVertex()));
      continue;
    }
    SmTArray<SmEdgeuse*> eus;
    lu->GetEdgeuses(eus);
    if (eus.GetSize() == 0) throw fail(SC_FAILED, "empty loop");
    o.push_back((double)eus.GetSize());
    for (ULONG k = 0; k < eus.GetSize(); k++) {
      auto it = cx.eix.find(eus[k]->GetEdge());
      if (it == cx.eix.end()) throw fail(SC_FAILED, "edgeuse of an unknown edge");
      o.push_back((double)it->second);
      o.push_back(eus[k]->GetOrientation() == SM_OT_SAME ? 1 : 0);
    }
  }
}

void write(SmBrep* b, std::vector<double>& o) {
  Ctx cx;
  cx.atol = approx_tol(b);
  o.push_back(SC_BREP_FORMAT);
  o.push_back(cx.atol);
  SmTArray<SmVertex*> vs;
  b->GetVertices(vs);
  o.push_back((double)vs.GetSize());
  for (ULONG i = 0; i < vs.GetSize(); i++) {
    cx.vix[vs[i]] = (uint32_t)i;
    SmPoint3d p = vs[i]->GetPoint();
    put3(o, p.x, p.y, p.z);
  }
  auto es = edges_of(b);
  for (size_t i = 0; i < es.size(); i++) cx.eix[es[i]] = (uint32_t)i;
  auto fs = faces_of(b);
  for (size_t i = 0; i < fs.size(); i++) cx.fix[fs[i]] = (uint32_t)i;
  o.push_back((double)es.size());
  for (SmEdge* e : es) put_edge(o, e, cx.vix, cx.atol);

  // bodies: one per solid region (outer shell(s) and voids), else one sheet body
  struct Body {
    std::vector<std::pair<SmShell*, bool>> shells;  // shell, inner
  };
  std::vector<Body> solids;
  SmTArray<SmRegion*> regions;
  b->GetRegions(regions);
  for (ULONG i = 0; i < regions.GetSize(); i++) {
    SmRegion* r = regions[i];
    if (r->IsInfiniteRegion() || r->IsVoid() != FALSE) continue;
    Body body;
    SmTArray<SmShell*> shells;
    r->GetShells(shells);
    for (ULONG k = 0; k < shells.GetSize(); k++) {
      SmShell* s = shells[k];
      if (!s->IsFaceuseShell()) continue;
      SmBoolean inner = FALSE;
      if (shells.GetSize() > 1 && s->IsInnerShell(inner) != SM_SUCCESS) inner = FALSE;
      body.shells.push_back({s, inner == TRUE});
    }
    if (!body.shells.empty()) solids.push_back(body);
  }
  if (!solids.empty()) {
    o.push_back((double)solids.size());
    for (Body& body : solids) {
      o.push_back(0);  // solid
      o.push_back((double)body.shells.size());
      for (auto& sh : body.shells) {
        SmTArray<SmFaceuse*> fus;
        sh.first->GetFaceuses(fus);
        o.push_back(sh.second ? 1 : 0);
        o.push_back((double)fus.GetSize());
        // the mate faceuse lies outside the material
        for (ULONG k = 0; k < fus.GetSize(); k++) put_face(o, cx, fus[k]->GetMate());
      }
    }
    return;
  }
  if (fs.empty()) {
    o.push_back(0);
    return;
  }
  o.push_back(1);
  o.push_back(1);  // sheet
  o.push_back(1);
  o.push_back(0);
  o.push_back((double)fs.size());
  for (SmFace* f : fs) {
    SmFaceuse* fu = f->GetUpwardFaceuse();
    if (!fu) throw fail(SC_FAILED, "face without faceuse");
    put_face(o, cx, fu);
  }
}

}  // namespace brep_out

// ================================================================== C ABI

extern "C" {

void sc_init(void) { ensure_init(); }

void sc_set_draft(int32_t on) { g_draft = on != 0; }
uint32_t sc_version(void) { return SC_FACADE_VERSION; }

int32_t sc_last_status(void) { return g_status; }
const char* sc_last_message(void) { return g_message.c_str(); }
uint32_t sc_last_message_len(void) { return (uint32_t)g_message.size(); }
const uint32_t* sc_last_edges(void) { return g_err_edges.data(); }
uint32_t sc_last_edges_len(void) { return (uint32_t)g_err_edges.size(); }

const double* sc_out_f64(uint32_t s) { return s < 8 ? g_f64[s].data() : nullptr; }
uint32_t sc_out_f64_len(uint32_t s) { return s < 8 ? (uint32_t)g_f64[s].size() : 0; }
const float* sc_out_f32(uint32_t s) { return s < 8 ? g_f32[s].data() : nullptr; }
uint32_t sc_out_f32_len(uint32_t s) { return s < 8 ? (uint32_t)g_f32[s].size() : 0; }
const uint32_t* sc_out_u32(uint32_t s) { return s < 8 ? g_u32[s].data() : nullptr; }
uint32_t sc_out_u32_len(uint32_t s) { return s < 8 ? (uint32_t)g_u32[s].size() : 0; }

void sc_retain(uint32_t h) {
  if (h > 0 && h < g_table.size() && g_table[h].brep) g_table[h].rc++;
}

void sc_release(uint32_t h) {
  if (h == 0 || h >= g_table.size() || !g_table[h].brep) return;
  if (--g_table[h].rc > 0) return;
  SmBrep* b = g_table[h].brep;
  g_table[h].brep = nullptr;
  g_free_slots.push_back(h);
  g_live--;
  try {
    delete b;
  } catch (...) {
  }
}

uint32_t sc_live_handles(void) { return g_live; }

#define SC_SHAPE_CALL(...)           \
  begin_call();                      \
  uint32_t _res = 0;                 \
  guarded([&] { __VA_ARGS__ });      \
  return _res;

#define SC_STATUS_CALL(...) \
  begin_call();             \
  guarded([&] { __VA_ARGS__ }); \
  return g_status;

// ---------------------------------------------------------------- primitives

uint32_t sc_make_box(double x0, double y0, double z0, double x1, double y1, double z1,
                     uint32_t tag) {
  SC_SHAPE_CALL({
    double dx = x1 - x0, dy = y1 - y0, dz = z1 - z0;
    if (!(dx > 0 && dy > 0 && dz > 0) || !is_fin(dx + dy + dz))
      throw fail(SC_INVALID, "box needs positive extents");
    SmBrep* b = nullptr;
    SC_CHECK(SmApiCreateBox(SmVector3d(x0, y0, z0), dx, dy, dz, b), "SmApiCreateBox");
    BrepPtr p(b);
    require_result(b, "SmApiCreateBox");
    tag_all(b, tag);
    _res = put(std::move(p));
  })
}

uint32_t sc_make_sphere(double r, uint32_t tag) {
  SC_SHAPE_CALL({
    if (!(r > 0) || !is_fin(r)) throw fail(SC_INVALID, "sphere radius must be > 0");
    SmBrep* b = nullptr;
    SC_CHECK(SmApiCreateSphere(SmVector3d(0, 0, 0), r, b), "SmApiCreateSphere");
    BrepPtr p(b);
    require_result(b, "SmApiCreateSphere");
    tag_all(b, tag);
    _res = put(std::move(p));
  })
}

uint32_t sc_make_cylinder(double h, double r1, double r2, uint32_t tag) {
  SC_SHAPE_CALL({
    if (!(h > 0) || !is_fin(h)) throw fail(SC_INVALID, "cylinder height must be > 0");
    if (r1 < 0 || r2 < 0 || !is_fin(r1) || !is_fin(r2) || (r1 <= 0 && r2 <= 0))
      throw fail(SC_INVALID, "cylinder radii must be >= 0 and not both 0");
    SmBrep* b = nullptr;
    bool flip = false;
    if (std::fabs(r1 - r2) <= 1e-12 * std::max(r1, r2)) {
      SC_CHECK(SmApiCreateCylinder(SmVector3d(0, 0, 0), r1, h, b), "SmApiCreateCylinder");
    } else if (r1 > 0) {
      SC_CHECK(SmApiCreateCone(SmVector3d(0, 0, 0), r1, r2, h, b), "SmApiCreateCone");
    } else {
      // tip at the bottom: build with the base at z = 0 and flip
      SC_CHECK(SmApiCreateCone(SmVector3d(0, 0, 0), r2, r1, h, b), "SmApiCreateCone");
      flip = true;
    }
    BrepPtr p(b);
    require_result(b, "cylinder");
    if (flip) {
      // rotate 180 degrees about x, then move up by h: (x, y, z) -> (x, -y, h - z)
      M4 m = identity();
      m.m[5] = -1;
      m.m[10] = -1;
      m.m[14] = h;
      apply_matrix(b, m);
    }
    tag_all(b, tag);
    _res = put(std::move(p));
  })
}

uint32_t sc_make_torus(double big_r, double r, uint32_t tag) {
  SC_SHAPE_CALL({
    if (!(r > 0) || !(big_r > r) || !is_fin(big_r) || !is_fin(r))
      throw fail(SC_INVALID, "torus needs R > r > 0");
    SmBrep* b = nullptr;
    SC_CHECK(SmApiCreateTorus(SmVector3d(0, 0, 0), big_r, r, b), "SmApiCreateTorus");
    BrepPtr p(b);
    require_result(b, "SmApiCreateTorus");
    tag_all(b, tag);
    _res = put(std::move(p));
  })
}

uint32_t sc_make_polyhedron(const double* points, uint32_t n_points, const uint32_t* face_sizes,
                            uint32_t n_faces, const uint32_t* indices, uint32_t n_indices,
                            uint32_t tag) {
  SC_SHAPE_CALL({
    if (n_points < 4 || n_faces < 4) throw fail(SC_INVALID, "polyhedron needs >= 4 points/faces");
    double diag = 0;
    {
      V3 mn = v3(1e300, 1e300, 1e300), mx = v3(-1e300, -1e300, -1e300);
      for (uint32_t i = 0; i < n_points; i++) {
        V3 p = v3(points[3 * i], points[3 * i + 1], points[3 * i + 2]);
        mn = v3(std::min(mn.x, p.x), std::min(mn.y, p.y), std::min(mn.z, p.z));
        mx = v3(std::max(mx.x, p.x), std::max(mx.y, p.y), std::max(mx.z, p.z));
      }
      diag = len(mx - mn);
    }
    SmPolyBrep* poly = new (*ctx()) SmPolyBrep(std::max(1e-9, 1e-9 * diag));
    ObjPtr<SmPolyBrep> polyOwner(poly);
    uint32_t k = 0;
    for (uint32_t f = 0; f < n_faces; f++) {
      uint32_t c = face_sizes[f];
      if (c < 3 || k + c > n_indices) throw fail(SC_INVALID, "bad polyhedron face");
      SmTArray<SmPoint3d> pts;
      for (uint32_t j = 0; j < c; j++) {
        uint32_t id = indices[k + j];
        if (id >= n_points) throw fail(SC_INVALID, "polyhedron index out of range");
        pts.Add(SmPoint3d(points[3 * id], points[3 * id + 1], points[3 * id + 2]));
      }
      k += c;
      SmPolyFace* pf = nullptr;
      SC_CHECK(poly->CreatePoly(nullptr, pts, pf), "SmPolyBrep::CreatePoly");
    }
    SmBrep* b = nullptr;
    SC_CHECK(SmBrep::CreateFromPolyBrep(*ctx(), *poly, b), "SmBrep::CreateFromPolyBrep");
    BrepPtr p(b);
    require_result(b, "polyhedron");
    if (!b->IsManifoldSolid())
      throw fail(SC_FAILED, "polyhedron faces do not form a closed manifold solid");
    SmBoolean maybeOpen = FALSE;
    b->OrientTrimmedSurfaces(TRUE, maybeOpen, FALSE);
    double vol = 0;
    if (SmApiBrepComputeVolume(b, 1e-6, vol) == SM_SUCCESS && vol < 0)
      throw fail(SC_FAILED, "polyhedron orientation could not be fixed");
    tag_all(b, tag);
    _res = put(std::move(p));
  })
}

// ---------------------------------------------------------------- operations

uint32_t sc_transform(uint32_t h, const double* m16) {
  SC_SHAPE_CALL({
    SmBrep* in = get(h);
    M4 m;
    std::memcpy(m.m, m16, sizeof(m.m));
    for (double v : m.m)
      if (!is_fin(v)) throw fail(SC_INVALID, "non-finite transform");
    if (std::fabs(m.m[3]) > 1e-15 || std::fabs(m.m[7]) > 1e-15 || std::fabs(m.m[11]) > 1e-15 ||
        std::fabs(m.m[15] - 1.0) > 1e-15)
      throw fail(SC_UNSUPPORTED, "projective transforms");
    if (is_identity(m)) {
      sc_retain(h);
      _res = h;
      return;
    }
    BrepPtr b = copy_brep(in);
    if (b->GetNumFaces() > 0) apply_matrix(b.get(), m);
    _res = put(std::move(b));
  })
}

uint32_t sc_boolean(int32_t op, const uint32_t* hs, uint32_t n, uint32_t tag) {
  SC_SHAPE_CALL({
    if (n == 0) throw fail(SC_INVALID, "boolean needs at least one shape");
    if (op < SC_UNION || op > SC_INTERSECTION) throw fail(SC_INVALID, "unknown boolean op");
    std::vector<const SmBrep*> inputs;
    for (uint32_t i = 0; i < n; i++) inputs.push_back(get(hs[i]));

    // Operand list: empty shapes drop out of a union, make an intersection empty and do
    // nothing as subtrahends.
    std::vector<const SmBrep*> ops;
    bool trivially_empty = false;
    for (uint32_t i = 0; i < n; i++) {
      bool empty = inputs[i]->GetNumFaces() == 0;
      if (i == 0 && op != SC_UNION) {
        if (empty) trivially_empty = true;
        ops.push_back(inputs[i]);
        continue;
      }
      if (empty) {
        if (op == SC_INTERSECTION) trivially_empty = true;
        continue;
      }
      ops.push_back(inputs[i]);
    }
    BrepPtr acc;
    if (trivially_empty || ops.empty()) {
      acc = empty_brep();
    } else if (ops.size() == 1) {
      acc = copy_brep(ops[0]);
    } else {
      // Union and intersection may combine the operands in any order, a difference its
      // subtrahends. If a step fails (SMLib error, wrong or open result, see
      // checked_boolean), the failing operand is moved forward and the chain is rebuilt;
      // only when no order works, the original order runs with the shifted retries and,
      // where nothing passes, keeps SMLib's own result (touching bodies, unreliable checks).
      size_t first_free = op == SC_DIFFERENCE ? 1 : 0;
      std::vector<std::vector<const SmBrep*>> tried;
      std::vector<const SmBrep*> order = ops;
      std::string err;
      // `last_resort`: shifted retries, and SMLib's own result is accepted if nothing passes
      auto run = [&](const std::vector<const SmBrep*>& ord, bool last_resort, size_t& failed_at) {
        BrepPtr a = copy_brep(ord[0]);
        double va = NAN;
        for (size_t i = 1; i < ord.size(); i++) {
          if (a->GetNumFaces() == 0) break;  // difference/intersection stays empty
          BrepPtr plain;
          bool impossible = false;
          BrepPtr r =
              checked_boolean(op, a.get(), ord[i], last_resort, err, plain, va, &impossible);
          if (!r.get() && last_resort && plain.get() && !impossible) {
            r = std::move(plain);
            va = NAN;
          }
          if (!r.get()) {
            failed_at = i;
            return BrepPtr();
          }
          a = std::move(r);
        }
        return a;
      };
      if (g_draft) {
        // Draft: SMLib's own results in the given order while every step succeeds (closed
        // results, or touching bodies merged); otherwise the checked path below.
        BrepPtr a = copy_brep(ops[0]);
        bool ok = true;
        for (size_t i = 1; i < ops.size() && ok; i++) {
          if (a->GetNumFaces() == 0) break;
          std::string e;
          BrepPtr r = raw_boolean(op, a.get(), ops[i], e);
          if (r.get() && r->GetNumFaces() > 0 && !r->IsManifoldSolid() && op == SC_UNION)
            r = touching_union(a.get(), ops[i]);
          if (!r.get() || (r->GetNumFaces() > 0 && !r->IsManifoldSolid())) ok = false;
          else a = std::move(r);
        }
        if (ok) acc = std::move(a);
      }
      for (int round = 0; round < 8 && !acc.get(); round++) {
        tried.push_back(order);
        size_t failed_at = 0;
        acc = run(order, false, failed_at);
        if (acc.get()) break;
        // next order: the failing operand one position earlier (not before first_free)
        bool found = false;
        for (size_t pos = failed_at; pos-- > first_free && !found;) {
          std::vector<const SmBrep*> next = order;
          const SmBrep* f = next[failed_at];
          next.erase(next.begin() + failed_at);
          next.insert(next.begin() + pos, f);
          if (std::find(tried.begin(), tried.end(), next) == tried.end()) {
            order = next;
            found = true;
          }
        }
        if (!found) break;
      }
      if (!acc.get()) {
        size_t failed_at = 0;
        acc = run(ops, true, failed_at);
      }
      if (!acc.get()) throw fail(SC_FAILED, err.empty() ? std::string("boolean failed") : err);
    }
    resolve_tags(acc.get(), inputs, tag, false);
    _res = put(std::move(acc));
  })
}

uint32_t sc_fillet(uint32_t h, const uint32_t* edges, uint32_t n, double r, uint32_t tag) {
  SC_SHAPE_CALL({ _res = do_fillet_like(h, edges, n, r, tag, false); })
}

uint32_t sc_chamfer(uint32_t h, const uint32_t* edges, uint32_t n, double d, uint32_t tag) {
  SC_SHAPE_CALL({ _res = do_fillet_like(h, edges, n, d, tag, true); })
}

int32_t sc_law_stations(uint32_t h, const uint32_t* edges, uint32_t n, const uint32_t* counts) {
  SC_STATUS_CALL({
    SmBrep* b = get(h);
    auto all = edges_of(b);
    auto& pts = g_f64[0];
    auto& ts = g_f64[1];
    for (uint32_t i = 0; i < n; i++) {
      if (edges[i] >= all.size())
        throw fail(SC_INVALID, "edge id " + std::to_string(edges[i]) + " out of range", {edges[i]});
      uint32_t m = counts[i];
      if (m < 2 || m > 256) throw fail(SC_INVALID, "a radius law needs 2 to 256 stations");
      SmEdge* e = all[edges[i]];
      SmCurve* c = e->GetCurve();
      if (!c) throw fail(SC_FAILED, "edge without curve", {edges[i]});
      SmExtent1d iv = e->GetInterval();
      for (double t : law_greville(m)) {
        SmPoint3d p;
        SC_CHECK(c->EvaluatePoint(iv.GetMin() + t * (iv.GetMax() - iv.GetMin()), p),
                 "SmCurve::EvaluatePoint");
        pts.push_back(p.x);
        pts.push_back(p.y);
        pts.push_back(p.z);
        ts.push_back(t);
      }
    }
  })
}

uint32_t sc_fillet_var(uint32_t h, const uint32_t* edges, uint32_t n, const uint32_t* counts,
                       const double* radii, uint32_t tag) {
  SC_SHAPE_CALL({
    std::unordered_map<uint32_t, std::vector<double>> laws;
    size_t k = 0;
    for (uint32_t i = 0; i < n; i++) {
      uint32_t m = counts[i];
      if (m < 1 || m > 256) throw fail(SC_INVALID, "a radius law needs 1 to 256 values");
      laws[edges[i]] = std::vector<double>(radii + k, radii + k + m);
      k += m;
    }
    _res = do_fillet_like(h, edges, n, 1.0, tag, false, &laws);
  })
}

uint32_t sc_extrude(const double* loop, uint32_t len_, double height, double twist,
                    double scale_x, double scale_y, uint32_t tag) {
  SC_SHAPE_CALL({
    auto loops = parse_loops(loop, len_, 1);
    const Loop& l = loops[0];
    if (!(std::fabs(height) > 0) || !is_fin(height))
      throw fail(SC_INVALID, "extrude height must be non-zero");
    if (!is_fin(twist) || !is_fin(scale_x) || !is_fin(scale_y) || scale_x < 0 || scale_y < 0)
      throw fail(SC_INVALID, "bad twist/scale");
    bool plain = std::fabs(twist) < 1e-12 && std::fabs(scale_x - 1) < 1e-12 &&
                 std::fabs(scale_y - 1) < 1e-12;
    BrepPtr b;
    if (plain) {
      CurveList cl;
      loop_curves(l, false, true, cl);
      SmBrep* r = nullptr;
      SC_CHECK(SmApiCreateLinearSweep(as_array(cl), SmVector3d(0, 0, height > 0 ? 1 : -1),
                                      std::fabs(height), TRUE, r),
               "SmApiCreateLinearSweep");
      require_result(r, "extrude");
      b.reset(r);
    } else {
      if (scale_x <= 1e-9 || scale_y <= 1e-9)
        throw fail(SC_UNSUPPORTED, "extrude with scale 0 (apex)");
      if (l.kind == SC_LOOP_CIRCLE && std::fabs(scale_x - scale_y) > 1e-12)
        throw fail(SC_UNSUPPORTED, "non-uniform scale of a circle profile");
      // Loft through k+1 sections. Section t (0..1) is the profile rotated by twist*t about
      // the origin and then scaled by lerp(1, scale, t) along x and y, in that order, as
      // OpenSCAD does (Eigen::Scaling(scale) * Rotation2D(angle) in its linear_extrude). For
      // a non-uniform scale the order matters: scaling first gives other cross-sections
      // (12_linear_extrude_twist: y extent 5.08 instead of 3.83).
      double step_deg = 7.5;
      int k = std::fabs(twist) < 1e-12 ? 1 : std::max(2, (int)std::ceil(std::fabs(twist) / (step_deg * M_PI / 180)));
      std::vector<CurveList> secs(k + 1);
      for (int i = 0; i <= k; i++) {
        double t = (double)i / k;
        double a = twist * t;
        double sx = 1 + (scale_x - 1) * t, sy = 1 + (scale_y - 1) * t;
        Loop s = l;
        s.frame = identity();
        // section frame: translate(0,0,h t) * scale(sx, sy) * rot(a)
        double c = std::cos(a), sn = std::sin(a);
        if (l.kind == SC_LOOP_CIRCLE || l.kind == SC_LOOP_COMPOSITE) {  // circles: sx == sy here
          s.frame.m[0] = c * sx;
          s.frame.m[1] = sn * sy;
          s.frame.m[4] = -sn * sx;
          s.frame.m[5] = c * sy;
          s.frame.m[10] = sx;
        } else {
          for (V3& p : s.pts) p = v3((c * p.x - sn * p.y) * sx, (sn * p.x + c * p.y) * sy, 0);
        }
        s.frame.m[14] = height * t;
        loop_curves(s, false, false, secs[i]);
      }
      std::vector<CurveList*> ptrs;
      for (auto& s : secs) ptrs.push_back(&s);
      b = skin(ptrs, k == 1 ? 1 : 3, "extrude (skin)");
    }
    require_solid(b.get(), "extrude");
    if (!is_identity(l.frame)) apply_matrix(b.get(), l.frame);
    tag_all(b.get(), tag);
    _res = put(std::move(b));
  })
}

uint32_t sc_revolve(const double* loop, uint32_t len_, double angle, uint32_t tag) {
  SC_SHAPE_CALL({
    auto loops = parse_loops(loop, len_, 1);
    if (!(std::fabs(angle) > 0) || !is_fin(angle))
      throw fail(SC_INVALID, "revolve angle must be non-zero");
    double deg = angle * 180.0 / M_PI;
    if (deg > 360.0) deg = 360.0;
    if (deg < -360.0) deg = -360.0;
    {
      // the profile must stay on one side of the z axis (x >= 0 after its frame)
      const Loop& l = loops[0];
      double scale = std::max(1.0, len(l.frame.col(0)) + len(l.frame.col(1)));
      double tol = 1e-9 * scale;
      double minx = 1e300;
      if (l.kind == SC_LOOP_CIRCLE) {
        double s;
        if (similarity(l.frame, s)) minx = l.frame.point(v3(l.cx, l.cy, 0)).x - l.r * s;
      } else {
        for (V3 p : l.pts) minx = std::min(minx, l.frame.point(p).x);
      }
      if (minx < -tol)
        throw fail(SC_INVALID, "revolve: the profile crosses the axis (x < 0 after its frame)");
    }
    CurveList cl;
    revolve_profile_curves(loops[0], cl);
    SmBrep* r = nullptr;
    SC_CHECK(SmApiCreateRotationalSweep(as_array(cl), SmPoint3d(0, 0, 0),
                                        SmVector3d(0, 0, deg > 0 ? 1 : -1), std::fabs(deg), TRUE,
                                        r),
             "SmApiCreateRotationalSweep");
    BrepPtr b(r);
    require_solid(r, "revolve");
    tag_all(b.get(), tag);
    _res = put(std::move(b));
  })
}

uint32_t sc_loft(const double* data, uint32_t len_, uint32_t n_sections, int32_t ruled,
                 uint32_t tag) {
  SC_SHAPE_CALL({
    auto loops = parse_loops(data, len_, n_sections);
    if (loops.size() < 2) throw fail(SC_INVALID, "loft needs at least 2 sections");
    bool segments = loft_segmentwise(loops);
    std::vector<CurveList> secs(loops.size());
    for (size_t i = 0; i < loops.size(); i++) loop_curves(loops[i], !segments, false, secs[i]);
    std::vector<CurveList*> ptrs;
    for (auto& s : secs) ptrs.push_back(&s);
    ULONG degree = ruled ? 1 : (ULONG)std::min<size_t>(3, loops.size() - 1);
    BrepPtr b = skin(ptrs, degree, "SmApiCreateSkinPrimitive");
    require_solid(b.get(), "loft");
    tag_all(b.get(), tag);
    _res = put(std::move(b));
  })
}

uint32_t sc_sweep(const double* loop, uint32_t len_, const double* path, uint32_t n_path,
                  int32_t interp, uint32_t tag) {
  SC_SHAPE_CALL({
    auto loops = parse_loops(loop, len_, 1);
    std::vector<V3> pp;
    for (uint32_t i = 0; i < n_path; i++) {
      V3 p = v3(path[3 * i], path[3 * i + 1], path[3 * i + 2]);
      if (!is_fin(p.x) || !is_fin(p.y) || !is_fin(p.z)) throw fail(SC_INVALID, "bad path point");
      if (!pp.empty() && len(p - pp.back()) < 1e-12) continue;
      pp.push_back(p);
    }
    if (pp.size() < 2) throw fail(SC_INVALID, "sweep path needs at least 2 distinct points");
    double plen = 0;
    for (size_t i = 1; i < pp.size(); i++) plen += len(pp[i] - pp[i - 1]);
    double ptol = 1e-9 * std::max(1.0, plen);
    // collinear path -> two points
    bool straight = true;
    {
      V3 d = normalize(pp.back() - pp[0]);
      for (V3 p : pp) {
        V3 q = p - pp[0];
        if (len(q - d * dot(q, d)) > ptol) straight = false;
      }
      if (straight) pp = {pp.front(), pp.back()};
    }
    // planar polyline?
    bool planar = true;
    V3 pn = v3(0, 0, 0);
    if (!straight) {
      for (size_t i = 1; i + 1 < pp.size() && len(pn) == 0; i++) {
        V3 c = cross(pp[i] - pp[0], pp[i + 1] - pp[0]);
        if (len(c) > ptol * plen) pn = normalize(c);
      }
      for (V3 p : pp)
        if (std::fabs(dot(p - pp[0], pn)) > 1e-7 * std::max(1.0, plen)) planar = false;
    }
    bool spline = interp == 1 && pp.size() >= 3 && !straight;
    if (!spline && !straight && !planar)
      throw fail(SC_UNSUPPORTED, "sweep along a non-planar polyline (use a spline path)");

    SmBSplineCurve* pathc = nullptr;
    if (spline) {
      SmCurve* c = interp_curve(pp, false);
      if (!c->IsKindOf(SmBSplineCurve_TYPE)) {
        delete c;
        throw fail(SC_FAILED, "interpolated path is not a B-spline");
      }
      pathc = (SmBSplineCurve*)c;
    } else {
      pathc = polyline_curve(pp, false);
    }
    ObjPtr<SmBSplineCurve> pathOwner(pathc);
    // start tangent
    V3 z = normalize(pp[1] - pp[0]);
    {
      SmExtent1d iv = pathc->GetNaturalInterval();
      SmVector3d pt, d1, d2;
      SmVector3d* ppt = &pt;
      SmVector3d* pd1 = &d1;
      SmVector3d* pd2 = nullptr;
      if (SmApiEvaluateCurve(pathc, iv.GetMin(), ppt, pd1, pd2) == SM_SUCCESS &&
          len(v3(d1.x, d1.y, d1.z)) > 0)
        z = normalize(v3(d1.x, d1.y, d1.z));
    }
    // profile x axis: global X projected into the normal plane (global Y if the tangent is
    // nearly parallel to X); y = z cross x.
    V3 ref = std::fabs(z.x) < 0.9 ? v3(1, 0, 0) : v3(0, 1, 0);
    V3 x = normalize(ref - z * dot(ref, z));
    V3 y = cross(z, x);
    Loop l = loops[0];
    l.frame = identity();
    l.frame.m[0] = x.x; l.frame.m[1] = x.y; l.frame.m[2] = x.z;
    l.frame.m[4] = y.x; l.frame.m[5] = y.y; l.frame.m[6] = y.z;
    l.frame.m[8] = z.x; l.frame.m[9] = z.y; l.frame.m[10] = z.z;
    l.frame.m[12] = pp[0].x; l.frame.m[13] = pp[0].y; l.frame.m[14] = pp[0].z;
    CurveList cl;
    loop_curves(l, false, false, cl);
    SmTArray<SmCurve*> prof = as_array(cl);
    // SMLib's curve sweep moves the profile's bounding-box centre along the path
    // (bMoveProfileCenterToPath), not the profile frame origin. That box is the box of the
    // control points: for a closed spline profile it is off-centre even where the curve is
    // symmetric about the origin ([[5, 0], [0, 3], [-5, 0], [0, -3]]: control box x -15..5).
    // smlib/patches/0003 adds bUseProfileRefPoint: the frame origin itself travels along the
    // path, centred or not. The mitered planar-polyline sweep has no such option: there an
    // offset is a translation on a straight path and not representable otherwise.
    SmExtent3d box;
    for (SmCurve* c : cl.v) {
      SmExtent3d cb;
      c->CalculateBoundingBox(c->GetNaturalInterval(), &cb);
      box.AddPoint3d(cb.GetMin());
      box.AddPoint3d(cb.GetMax());
    }
    double psize = len(fromsm(box.GetMax()) - fromsm(box.GetMin()));
    bool centred = len(fromsm(box.GetMid()) - pp[0]) <= 1e-9 * std::max(1.0, psize);
    bool curve_sweep = straight || spline;
    if (!centred && !curve_sweep)
      throw fail(SC_UNSUPPORTED,
                 "sweep of a profile that is not centred on its origin along a polyline path "
                 "with corners (use a spline path)");
    SmBrep* r = nullptr;
    if (!straight && !spline) {
      // planar polyline: mitered joints
      CurveList segs;
      for (size_t i = 0; i + 1 < pp.size(); i++) {
        SmLine* ln = nullptr;
        SC_CHECK(SmApiCreateLineSegment(sp(pp[i]), sp(pp[i + 1]), ln), "SmApiCreateLineSegment");
        segs.add(ln);
      }
      SmTArray<SmCurve*> segarr = as_array(segs);
      SC_CHECK(SmApiCreateSweepAlongPlanarPath(prof, segarr, TRUE, 3, r),
               "SmApiCreateSweepAlongPlanarPath");
    } else {
      SmSweepOptions opt;
      opt.bCapEndsArg = TRUE;
      opt.bMoveProfileCenterToPath = TRUE;
      opt.bUseProfileRefPoint = TRUE;
      opt.sProfileRefPoint = SmVector3d(pp[0].x, pp[0].y, pp[0].z);
      SC_CHECK(SmApiCreateCurveSweep(prof, pathc, nullptr, nullptr, &opt, r, nullptr, nullptr,
                                     nullptr),
               "SmApiCreateCurveSweep");
    }
    BrepPtr b(r);
    if (r && r->GetNumFaces() > 0 && !r->IsManifoldSolid() && loops[0].kind == SC_LOOP_POLYGON &&
        !straight) {
      // SMLib's curve sweep sometimes leaves seams between neighbouring side faces of a
      // polygon profile open: coincident twin edges (gap 0) that SmBrep::StitchFaces does not
      // glue either (16-gon along a curved path; 12-, 32-gons on other paths).
      size_t open = 0;
      for (SmEdge* e : edges_of(r)) {
        SmTArray<SmFace*> ef;
        e->GetFaces(ef);
        if (ef.GetSize() < 2) open++;
      }
      throw fail(SC_FAILED, "sweep: SMLib left " + std::to_string(open / 2) +
                                " seam(s) between the side faces of the polygon profile open "
                                "(SMLib limitation on curved paths; circle() or a spline() "
                                "profile work)");
    }
    require_solid(r, "sweep");
    tag_all(b.get(), tag);
    _res = put(std::move(b));
  })
}

uint32_t sc_nurbs_surface(const double* pts, uint32_t rows, uint32_t cols, uint32_t tag) {
  SC_SHAPE_CALL({
    if (rows < 2 || cols < 2) throw fail(SC_INVALID, "surface needs at least 2x2 points");
    SmTArray<SmPoint3d> p;
    for (uint32_t i = 0; i < rows * cols; i++)
      p.Add(SmPoint3d(pts[3 * i], pts[3 * i + 1], pts[3 * i + 2]));
    // Interpolating B-spline (SMLib's least-squares fit with as many control points as points),
    // degree 3 per direction where it has at least 4 points, otherwise one less than its point
    // count: bilinear for 2, quadratic for 3 (SmApiCreateSurfaceFromOrderedPoints is bicubic
    // only and fails below 4 x 4).
    ULONG du = std::min<ULONG>(3, rows - 1), dv = std::min<ULONG>(3, cols - 1);
    SmBSplineSurface* bs = nullptr;
    SC_CHECK(SmBSplineSurface::ApproximatePoints(*ctx(), p, rows, cols, du, dv, nullptr, bs, true),
             "SmBSplineSurface::ApproximatePoints");
    SmSurface* s = bs;
    if (!s) throw fail(SC_FAILED, "no surface created");
    BrepPtr b = empty_brep();
    SmFace* f = nullptr;
    SmStatus st = b->CreateFaceFromSurface(s, s->GetNaturalUVDomain(), f);
    if (st != SM_SUCCESS || !f) {
      s->SetOwner(nullptr);
      delete s;
      throw sm_fail("SmBrep::CreateFaceFromSurface", st == SM_SUCCESS ? SM_ERR : st);
    }
    tag_all(b.get(), tag);
    _res = put(std::move(b));
  })
}

uint32_t sc_thicken(uint32_t h, double t, uint32_t tag) {
  SC_SHAPE_CALL({
    SmBrep* in = get(h);
    if (!(std::fabs(t) > 0) || !is_fin(t)) throw fail(SC_INVALID, "thickness must be non-zero");
    if (in->IsManifoldSolid()) throw fail(SC_INVALID, "thicken expects an open surface");
    SmTArray<const SmFace*> none;
    SmBrep* r = nullptr;
    SC_CHECK(SmApiShellBrepFull(in, t, TRUE, TRUE, TRUE, none, r), "SmApiShellBrepFull");
    require_result(r, "thicken");
    BrepPtr b(r);
    check_valid(b.get(), "thicken");
    resolve_tags(b.get(), {in}, tag, true);
    _res = put(std::move(b));
  })
}

// Smallest radius of convex curvature over the faces of `b` that are not in `skip` (finite
// differences of the outward normal on a 7 x 7 grid of each face's UV domain; INFINITY if none
// is convex). An inward shell offsets every face by t: where a convex rounding has a radius
// below t the offset surface degenerates, and SMLib's shell fails with a bare SM_ERR.
double min_convex_radius(const SmBrep* b, const std::vector<const SmFace*>& skip) {
  double best = INFINITY;
  for (SmFace* f : faces_of(b)) {
    if (std::find(skip.begin(), skip.end(), f) != skip.end()) continue;
    SmSurface* srf = f->GetSurface();
    if (!srf || srf->IsKindOf(SmPlane_TYPE)) continue;
    SmFaceuse* fu = f->GetUpwardFaceuse();
    bool flip = fu && fu->GetShell() && fu->GetShell()->GetRegion() &&
                !fu->GetShell()->GetRegion()->IsVoid();
    SmExtent2d d = f->GetUVDomain();
    double du = 1e-4 * (d.GetUMax() - d.GetUMin()), dv = 1e-4 * (d.GetVMax() - d.GetVMin());
    auto eval = [&](double u, double v, V3& p, V3& nrm) {
      SmVector2d uv(u, v);
      SmVector3d pv, nv;
      if (SmApiEvaluateSurfacePoint(srf, uv, pv) != SM_SUCCESS) return false;
      if (SmApiEvaluateSurfaceNormal(srf, uv, nv) != SM_SUCCESS) return false;
      p = v3(pv.x, pv.y, pv.z);
      nrm = normalize(v3(nv.x, nv.y, nv.z));
      if (flip) nrm = nrm * -1.0;
      return len(nrm) > 0;
    };
    for (int i = 1; i <= 7; i++)
      for (int j = 1; j <= 7; j++) {
        double u = d.GetUMin() + (d.GetUMax() - d.GetUMin()) * i / 8.0;
        double v = d.GetVMin() + (d.GetVMax() - d.GetVMin()) * j / 8.0;
        SmPointClassification pc;
        if (f->PointClassify(SmPoint2d(u, v), SmTol::GetZoneTol3d(f), FALSE, TRUE, pc) != SM_SUCCESS ||
            pc.GetPointClass() != SM_PC_FACE)
          continue;
        V3 p0, n0;
        if (!eval(u, v, p0, n0)) continue;
        for (int k = 0; k < 2; k++) {
          V3 p1, n1;
          if (!eval(u + (k == 0 ? du : 0), v + (k == 1 ? dv : 0), p1, n1)) continue;
          V3 dp = p1 - p0;
          double l2 = dot(dp, dp);
          if (l2 <= 0) continue;
          double kappa = dot(n1 - n0, dp) / l2;  // > 0: convex in this direction
          if (kappa > 1e-9) best = std::min(best, 1.0 / kappa);
        }
      }
  }
  return best;
}

uint32_t sc_shell(uint32_t h, double t, const uint32_t* open_faces, uint32_t n, uint32_t tag) {
  SC_SHAPE_CALL({
    SmBrep* in = get(h);
    if (!(t > 0) || !is_fin(t)) throw fail(SC_INVALID, "shell thickness must be > 0");
    auto fs = faces_of(in);
    SmTArray<const SmFace*> open;
    for (uint32_t i = 0; i < n; i++) {
      if (open_faces[i] >= fs.size())
        throw fail(SC_INVALID, "face id " + std::to_string(open_faces[i]) + " out of range");
      open.Add(fs[open_faces[i]]);
    }
    std::vector<const SmFace*> open_v;
    for (ULONG i = 0; i < open.GetSize(); i++) open_v.push_back(open[i]);
    double gap = 0;
    if (opposite_walls_closer_than(in, open_v, 2.0 * t, gap))
      throw fail(SC_FAILED, "shell: wall thickness " + std::to_string(t) +
                                " does not fit (opposite walls " + std::to_string(gap) + " apart)");
    // SMLib's shell first; where it fails or its result is not valid, the shell from Booleans
    // (see shell_by_booleans).
    BrepPtr b;
    Failure first{SC_OK, "", {}};
    try {
      SmBrep* r = nullptr;
      SC_CHECK(SmApiShellBrepFull(in, -t, TRUE, TRUE, FALSE, open, r), "SmApiShellBrepFull");
      require_result(r, "shell");
      b.reset(r);
      if (b->GetNumFaces() == 0 || !b->IsManifoldSolid())
        throw fail(SC_FAILED, "shell: wall thickness " + std::to_string(t) +
                                  " leaves no cavity (SMLib returned no closed solid)");
      check_valid(b.get(), "shell");
    } catch (const Failure& f) {
      b.reset();
      first = f;
    }
    if (!b.get()) {
      std::string why;
      b = shell_by_booleans(in, t, open_v, why);
      if (!b.get()) {
        first.msg += "; shell from booleans: " + why;
        double rmin = min_convex_radius(in, open_v);
        if (rmin <= 1.02 * t)
          first.msg = "shell: wall thickness " + std::to_string(t) +
                      " is not smaller than the radius of a convex rounding of the body (about " +
                      std::to_string(rmin) +
                      "): the inner surface degenerates there (rounding radius must be > wall "
                      "thickness). " + first.msg;
        throw first;
      }
    }
    std::vector<const SmFace*> removed;
    for (ULONG i = 0; i < open.GetSize(); i++) removed.push_back(open[i]);
    resolve_tags(b.get(), {in}, tag, true, removed);
    _res = put(std::move(b));
  })
}

// ---------------------------------------------------------------- queries

int32_t sc_edges(uint32_t h, uint32_t samples) {
  SC_STATUS_CALL({
    SmBrep* b = get(h);
    auto& pts = g_f64[0];
    auto& meta = g_u32[0];
    for (SmEdge* e : edges_of(b)) {
      uint32_t np = samples + 2;
      SmCurve* c = e->GetCurve();
      SmExtent1d iv = e->GetInterval();
      bool ok = c != nullptr;
      std::vector<V3> ep;
      for (uint32_t k = 0; ok && k < np; k++) {
        double t = iv.GetMin() + (iv.GetMax() - iv.GetMin()) * (double)k / (np - 1);
        SmPoint3d p;
        if (c->EvaluatePoint(t, p) != SM_SUCCESS) {
          ok = false;
          break;
        }
        ep.push_back(fromsm(p));
      }
      if (!ok) {
        ep.clear();
        SmVertex* a = e->GetStartVertex();
        SmVertex* z = e->GetEndVertex();
        V3 pa = a ? fromsm(a->GetPoint()) : v3(0, 0, 0);
        V3 pz = z ? fromsm(z->GetPoint()) : pa;
        ep.push_back(pa);
        ep.push_back(pz);
      }
      for (V3 p : ep) {
        pts.push_back(p.x);
        pts.push_back(p.y);
        pts.push_back(p.z);
      }
      SmTArray<SmFace*> ef;
      e->GetFaces(ef);
      std::vector<uint32_t> tags;
      for (ULONG i = 0; i < ef.GetSize(); i++) {
        uint32_t t = 0;
        face_tag(ef[i], t);
        tags.push_back(t);
      }
      if (ef.GetSize() == 1 && e->GetEdgeuseCount() >= 4) tags.push_back(tags[0]);  // seam
      // SC_EDGE_SMOOTH: nothing to blend (seam, pole, tangent-continuous or open edge;
      // GetBlendEdgeuse() is NULL for exactly these, see do_fillet_like).
      uint32_t flags = 0;
      if (e->GetBlendEdgeuse() == nullptr) flags |= SC_EDGE_SMOOTH;
      meta.push_back((uint32_t)ep.size());
      meta.push_back((uint32_t)tags.size());
      meta.push_back(flags);
      for (uint32_t t : tags) meta.push_back(t);
    }
  })
}

int32_t sc_faces(uint32_t h, uint32_t samples) {
  SC_STATUS_CALL({
    SmBrep* b = get(h);
    auto& pts = g_f64[0];
    auto& meta = g_u32[0];
    uint32_t n = std::max<uint32_t>(samples, 1);
    for (SmFace* f : faces_of(b)) {
      uint32_t t = 0;
      face_tag(f, t);
      std::vector<V3> fp;
      V3 c, nn;
      if (face_sample(f, c, nn)) fp.push_back(c);
      // interior grid
      SmExtent2d d = f->GetUVDomain();
      SmSurface* srf = f->GetSurface();
      for (uint32_t i = 0; srf && i < n; i++)
        for (uint32_t j = 0; j < n; j++) {
          double s = (i + 0.5) / n, tt = (j + 0.5) / n;
          SmPoint2d uv(d.GetUMin() + s * (d.GetUMax() - d.GetUMin()),
                       d.GetVMin() + tt * (d.GetVMax() - d.GetVMin()));
          SmPointClassification pc;
          if (f->PointClassify(uv, SmTol::GetZoneTol3d(f), FALSE, TRUE, pc) != SM_SUCCESS ||
              pc.GetPointClass() != SM_PC_FACE)
            continue;
          SmVector3d pv;
          if (SmApiEvaluateSurfacePoint(srf, SmVector2d(uv.x, uv.y), pv) == SM_SUCCESS)
            fp.push_back(v3(pv.x, pv.y, pv.z));
        }
      // boundary
      SmTArray<SmEdge*> fe;
      f->GetEdges(fe);
      for (ULONG k = 0; k < fe.GetSize(); k++) {
        SmCurve* cv = fe[k]->GetCurve();
        if (!cv) continue;
        SmExtent1d iv = fe[k]->GetInterval();
        for (uint32_t s = 0; s <= n; s++) {
          SmPoint3d p;
          double tt = iv.GetMin() + (iv.GetMax() - iv.GetMin()) * (double)s / n;
          if (cv->EvaluatePoint(tt, p) == SM_SUCCESS) fp.push_back(fromsm(p));
        }
      }
      for (V3 p : fp) {
        pts.push_back(p.x);
        pts.push_back(p.y);
        pts.push_back(p.z);
      }
      meta.push_back(t);
      meta.push_back((uint32_t)fp.size());
    }
  })
}

int32_t sc_classify(uint32_t h, const double* p, uint32_t n) {
  SC_STATUS_CALL({
    SmBrep* b = get(h);
    auto& out = g_u32[0];
    for (uint32_t i = 0; i < n; i++) {
      int c = classify_point(b, v3(p[3 * i], p[3 * i + 1], p[3 * i + 2]));
      if (c < 0) throw sm_fail("SmBrep::Point3DClassify", SM_ERR);
      out.push_back(c == 1 ? SC_INSIDE : c == 2 ? SC_ON : SC_OUTSIDE);
    }
  })
}

int32_t sc_tessellate(uint32_t h, double tol) {
  SC_STATUS_CALL({
    SmBrep* b = get(h);
    if (!(tol > 0) || !is_fin(tol)) throw fail(SC_INVALID, "tolerance must be > 0");
    if (b->GetNumFaces() == 0) return;
    auto fs = faces_of(b);
    auto es = edges_of(b);
    std::unordered_map<const SmFace*, uint32_t> face_ix;
    for (size_t i = 0; i < fs.size(); i++) face_ix[fs[i]] = (uint32_t)i;
    std::unordered_map<const SmEdge*, uint32_t> edge_ix;
    for (size_t i = 0; i < es.size(); i++) edge_ix[es[i]] = (uint32_t)i;
    std::vector<uint32_t> tags(fs.size());
    for (size_t i = 0; i < fs.size(); i++) face_tag(fs[i], tags[i]);

    SmTessellationParams prm;
    prm.dChordHeightTol = tol;
    prm.dCurveAngleTolDeg = 20.0;
    prm.dSurfaceAngleTolDeg = 20.0;
    auto run_tess = [&]() {
      SmPolyBrep* p = nullptr;
      SmTArray<SmTessellationFailure> failures;
      SmStatus st = SmApiTessellate(b, p, prm, TRUE, &failures, nullptr);
      if (st != SM_SUCCESS || !p) {
        if (p) delete p;
        throw sm_fail("SmApiTessellate", st == SM_SUCCESS ? SM_ERR : st);
      }
      return p;
    };
    SmPolyBrep* poly = run_tess();
    // SMLib bounds the chord height only along the triangle edges. On doubly curved free-form
    // faces with a twist (linear_extrude with twist, lofts, sweeps) long thin triangles whose
    // edges lie on the surface still cut through it: twisted 8 mm square, 90 degrees over
    // 20 mm: 0.2 mm in the triangle centres, mesh volume +2.9 %. Measure the centroid deviation
    // on free-form faces and re-tessellate with a maximum edge length until it is within
    // 2 * tol (planar and analytic faces are left to SMLib; their error is the chord height).
    // Each pass sets a global maximum edge length (planar faces get finer too), at least
    // halving it; at most 3 passes, and none beyond a budget of predicted triangles.
    {
      double area = 0;
      size_t n0 = poly_triangles(poly, &area);
      size_t budget = std::min<size_t>(300000, std::max<size_t>(20000, 8 * n0));
      for (int pass = 0; pass < 3; pass++) {
        double worst = 0, worst_edge = 0;
        tess_deviation(poly, face_ix, worst, worst_edge);
        if (worst <= 2.0 * tol || worst_edge <= 0) break;
        double target = worst_edge * std::sqrt(tol / worst) * 0.9;
        if (prm.dMaxEdgeLength > 0) target = std::min(target, 0.5 * prm.dMaxEdgeLength);
        // about area / (0.43 target^2) triangles at that edge length
        double floor_len = std::sqrt(area / (0.43 * (double)budget));
        if (target < floor_len) target = floor_len;
        if (target >= 0.9 * worst_edge) break;  // budget leaves no room for improvement
        prm.dMaxEdgeLength = target;
        SmPolyBrep* finer = run_tess();
        delete poly;
        poly = finer;
      }
    }
    ObjPtr<SmPolyBrep> polyOwner(poly);

    auto& pos = g_f32[0];
    auto& nor = g_f32[1];
    auto& seg = g_f32[2];
    auto& idx = g_u32[0];
    auto& ttag = g_u32[1];
    auto& tface = g_u32[2];
    auto& eid = g_u32[3];
    std::unordered_map<std::pair<const void*, uint32_t>, uint32_t, PairHash> vmap;
    std::map<std::pair<const void*, const void*>, std::vector<uint32_t>> medge;
    std::unordered_map<const void*, V3> pvpos;

    SmTArray<SmPolyFace*> pfs;
    poly->GetPolyFaces(pfs);
    for (ULONG i = 0; i < pfs.GetSize(); i++) {
      SmPolyFace* pf = pfs[i];
      SmFace* of = pf->GetOriginalFace();
      auto fit = face_ix.find(of);
      if (fit == face_ix.end()) continue;
      uint32_t fi = fit->second;
      SmTArray<SmPolyEdge*> pe;
      pf->GetPolyEdges(pe);
      std::vector<uint32_t> vids;
      std::vector<V3> vpos, vnor;
      std::vector<const void*> vpv;
      for (ULONG j = 0; j < pe.GetSize(); j++) {
        SmPolyVertex* pv = pe[j]->GetStartPolyVertex();
        V3 p = fromsm(pv->GetPoint());
        // normal of this vertex on this face
        V3 n = v3(0, 0, 0);
        SmTArray<SmTopology*>& vf = pv->GetFacesRef();
        SmTArray<SmVector3d>& vn = pv->GetNormalsRef();
        for (ULONG k = 0; k < vf.GetSize() && k < vn.GetSize(); k++)
          if (vf[k] == (SmTopology*)of) {
            n = normalize(v3(vn[k].x, vn[k].y, vn[k].z));
            break;
          }
        auto key = std::make_pair((const void*)pv, fi);
        auto it = vmap.find(key);
        uint32_t vid;
        if (it == vmap.end()) {
          vid = (uint32_t)(pos.size() / 3);
          vmap.emplace(key, vid);
          pos.push_back((float)p.x);
          pos.push_back((float)p.y);
          pos.push_back((float)p.z);
          nor.push_back((float)n.x);
          nor.push_back((float)n.y);
          nor.push_back((float)n.z);
        } else {
          vid = it->second;
        }
        vids.push_back(vid);
        vpos.push_back(p);
        vnor.push_back(n);
        vpv.push_back(pv);
        pvpos[pv] = p;
      }
      if (vids.size() < 3) continue;
      // fan triangulation (SMLib emits triangles; larger polygons are convex in practice)
      for (size_t k = 1; k + 1 < vids.size(); k++) {
        V3 tn = cross(vpos[k] - vpos[0], vpos[k + 1] - vpos[0]);
        V3 an = vnor[0] + vnor[k] + vnor[k + 1];
        bool flip = dot(tn, an) < 0;
        idx.push_back(vids[0]);
        idx.push_back(flip ? vids[k + 1] : vids[k]);
        idx.push_back(flip ? vids[k] : vids[k + 1]);
        ttag.push_back(tags[fi]);
        tface.push_back(fi);
        const void* tp[3] = {vpv[0], vpv[k], vpv[k + 1]};
        for (int q = 0; q < 3; q++) {
          const void* a = tp[q];
          const void* c = tp[(q + 1) % 3];
          if (a == c) continue;
          if (a > c) std::swap(a, c);
          medge[std::make_pair(a, c)].push_back(fi);
        }
      }
    }
    emit_edge_segments(b, fs, es, face_ix, edge_ix, medge, pvpos, seg, eid);
    // vertices without a face normal: use the triangle normals
    for (size_t v = 0; v < nor.size() / 3; v++) {
      if (nor[3 * v] != 0 || nor[3 * v + 1] != 0 || nor[3 * v + 2] != 0) continue;
      for (size_t t = 0; t < idx.size(); t += 3) {
        if (idx[t] != v && idx[t + 1] != v && idx[t + 2] != v) continue;
        V3 a = v3(pos[3 * idx[t]], pos[3 * idx[t] + 1], pos[3 * idx[t] + 2]);
        V3 bb = v3(pos[3 * idx[t + 1]], pos[3 * idx[t + 1] + 1], pos[3 * idx[t + 1] + 2]);
        V3 c = v3(pos[3 * idx[t + 2]], pos[3 * idx[t + 2] + 1], pos[3 * idx[t + 2] + 2]);
        V3 n = normalize(cross(bb - a, c - a));
        nor[3 * v] = (float)n.x;
        nor[3 * v + 1] = (float)n.y;
        nor[3 * v + 2] = (float)n.z;
        break;
      }
    }
  })
}

int32_t sc_mass_props(uint32_t h) {
  SC_STATUS_CALL({
    SmBrep* b = get(h);
    auto& out = g_f64[0];
    if (b->GetNumFaces() == 0) {
      out = {0, 0, 0, 0, 0};
      return;
    }
    if (b->IsManifoldSolid()) {
      double area = 0, vol = 0, mass = 0;
      SmPoint3d c;
      SmVector3d mi, pi;
      SC_CHECK(SmApiBrepComputeMassProperties(b, 1e-4, 1.0, SmPoint3d(0, 0, 0), area, vol, mass, c,
                                              mi, pi),
               "SmApiBrepComputeMassProperties");
      double fine = vol;
      if (SmApiBrepComputeVolume(b, 1e-6, fine) == SM_SUCCESS && is_fin(fine)) vol = fine;
      out = {vol, area, c.x, c.y, c.z};
    } else {
      double area = 0;
      SC_CHECK(SmApiBrepComputeArea(b, 1e-4, area), "SmApiBrepComputeArea");
      SmPoint3d mn, mx;
      SC_CHECK(SmApiBrepBoundingBox(b, FALSE, mn, mx), "SmApiBrepBoundingBox");
      out = {0, area, (mn.x + mx.x) / 2, (mn.y + mx.y) / 2, (mn.z + mx.z) / 2};
    }
  })
}

int32_t sc_bounds(uint32_t h) {
  SC_STATUS_CALL({
    SmBrep* b = get(h);
    if (b->GetNumFaces() == 0) return;
    SmPoint3d mn, mx;
    SC_CHECK(SmApiBrepBoundingBox(b, TRUE, mn, mx), "SmApiBrepBoundingBox");
    g_f64[0] = {mn.x, mn.y, mn.z, mx.x, mx.y, mx.z};
  })
}

int32_t sc_is_empty(uint32_t h) {
  begin_call();
  int32_t r = -1;
  guarded([&] { r = get(h)->GetNumFaces() == 0 ? 1 : 0; });
  return r;
}

int32_t sc_is_solid(uint32_t h) {
  begin_call();
  int32_t r = -1;
  guarded([&] {
    SmBrep* b = get(h);
    r = (b->GetNumFaces() > 0 && b->IsManifoldSolid()) ? 1 : 0;
  });
  return r;
}

int32_t sc_face_count(uint32_t h) {
  begin_call();
  int32_t r = -1;
  guarded([&] { r = (int32_t)get(h)->GetNumFaces(); });
  return r;
}

int32_t sc_edge_count(uint32_t h) {
  begin_call();
  int32_t r = -1;
  guarded([&] { r = (int32_t)edges_of(get(h)).size(); });
  return r;
}

int32_t sc_brep(uint32_t h) {
  SC_STATUS_CALL({
    SmBrep* b = get(h);
    brep_out::write(b, g_f64[0]);
  })
}

}  // extern "C"

// sketches: composite loops, sweep along a sketch path
#define SC_FACADE_INTERNAL 1
#include "sc_sketch.cpp"
