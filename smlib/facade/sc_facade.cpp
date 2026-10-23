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

#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <exception>
#include <map>
#include <new>
#include <string>
#include <unordered_map>
#include <vector>

#include "sc_facade.h"

namespace {

const ULONG SC_TAG_ATTR_ID = SM_AI_USER_1 + 77;  // 10077
const uint32_t SC_FACADE_VERSION = 1;

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
};

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
  SC_CHECK(b->Transform(t, TRUE), "SmBrep::Transform");
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

uint32_t do_fillet_like(uint32_t h, const uint32_t* ids, uint32_t n, double r, uint32_t tag,
                        bool chamfer) {
  const char* what = chamfer ? "chamfer" : "fillet";
  if (!(r > 0) || !is_fin(r)) throw fail(SC_INVALID, std::string(what) + " size must be > 0");
  SmBrep* in = get(h);
  std::vector<SmEdge*> in_all;
  std::vector<SmEdge*> in_sel = select_edges(in, ids, n, in_all);
  BrepPtr b = copy_brep(in);
  std::vector<SmEdge*> all = edges_of(b.get());
  if (all.size() != in_all.size()) throw fail(SC_FAILED, "Brep copy changed the edge count");
  SmTArray<SmEdge*> sel;
  std::vector<uint32_t> sel_ids;
  std::vector<SmEdge*> kept_in;
  std::vector<uint32_t> skipped;
  for (SmEdge* e : in_sel) {
    uint32_t i = edge_index(in_all, e);
    // GetBlendEdgeuse() is NULL for edges SMLib cannot blend: seams and other
    // tangent-continuous edges, lamina edges. Nothing to round there: skip them.
    if (all[i]->GetBlendEdgeuse() == nullptr) {
      skipped.push_back(i);
      continue;
    }
    sel.Add(all[i]);
    sel_ids.push_back(i);
    kept_in.push_back(e);
  }
  in_sel = kept_in;
  if (sel.GetSize() == 0)
    throw fail(SC_FAILED,
               std::string(what) + ": none of the edges can be blended (smooth, seam or open edges)",
               skipped);

  SmFilletExecutive ex(*ctx(), b.get());
  ex.SetDoGlobalMerge(TRUE);
  SmStatus st = ex.SetFilletParameters(b.get(), sel, chamfer ? SM_FSG_LINEAR : SM_FSG_CIRCULAR,
                                       SM_FS_CONST_RADIUS, r, nullptr, -1.0, -1.0, -1.0, -1.0, 1,
                                       1.0);
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
      uint32_t i = edge_index(all, info->m_pFilletedEdge);
      if (i != UINT32_MAX) bad.push_back(i);
    }
    throw sm_fail(what, st, bad);
  }
  b->StitchAndOrient();
  if (b->GetNumFaces() == 0) throw fail(SC_FAILED, std::string(what) + " removed all faces");
  auto badf = invalid_faces(b.get());
  if (!badf.empty()) {
    auto e = edges_near_faces(b.get(), badf, in_sel, in_all, 3.0 * r + 1e-6);
    if (e.empty()) e = sel_ids;
    throw fail(SC_FAILED,
               std::string(what) + " failed: the result overlaps itself (size " +
                   std::to_string(r) + " too large for the adjacent faces?)",
               e);
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

Plausible boolean_plausible(int32_t op, const SmBrep* A, const SmBrep* B, const SmBrep* R,
                            std::string& why) {
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

  double va = solid_volume(A), vb = solid_volume(B), vr = r_empty ? 0.0 : solid_volume(R);
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

// A op B with the plausibility check and fallbacks (see above). `allow_perturb` = false
// leaves out the shifted retries (used while other operand orders are still untried); they
// are only tried after a wrong result or an SMLib error, not for results that are merely
// open. Returns nullptr if nothing passes; `plain` then holds SMLib's own result for A op B
// (if it produced one), which the caller takes as a last resort: the check itself relies on
// SMLib's point classification, which errs on some bodies (revolved splines), so a result
// it rejects is not necessarily wrong, and it is what the facade returned before the check.
BrepPtr checked_boolean(int32_t op, const SmBrep* A, const SmBrep* B, bool allow_perturb,
                        std::string& err, BrepPtr& plain) {
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
    Plausible pl = boolean_plausible(op, a, b, r.get(), w);
    if (pl == PL_OK) return r;
    if (pl == PL_WRONG) wrong = true;
    if (is_plain) plain = std::move(r);
    if (why.empty()) why = "SMLib returned a wrong result (" + w + ")";
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

// SMLib's inward shell reads freed memory when the wall does not fit (valgrind: invalid reads
// in SmProtoEdge::RemoveProtoVertex for a 10 mm cube with t >= 5; solidcore
// plan/upstream-issues.md UB-239, UB-242). Depending on the heap the call then fails, returns
// a solid or crashes the process (kernel.rs::invalid_profiles_fail_cleanly: 1 of 100 runs
// with 8 threads, after rand() was fixed). This catches the common case before SMLib: two
// antiparallel planar walls (not opened) facing each other at a distance below `limit`
// (= 2 t). Curved walls are not checked.
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

// ================================================================== C ABI

extern "C" {

void sc_init(void) { ensure_init(); }
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
        for (size_t i = 1; i < ord.size(); i++) {
          if (a->GetNumFaces() == 0) break;  // difference/intersection stays empty
          BrepPtr plain;
          BrepPtr r = checked_boolean(op, a.get(), ord[i], last_resort, err, plain);
          if (!r.get() && last_resort && plain.get()) r = std::move(plain);
          if (!r.get()) {
            failed_at = i;
            return BrepPtr();
          }
          a = std::move(r);
        }
        return a;
      };
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
        if (l.kind == SC_LOOP_CIRCLE) {  // sx == sy here
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
    loop_curves(loops[0], false, false, cl);
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
    bool segments = true;
    for (const Loop& l : loops)
      if (l.kind != SC_LOOP_POLYGON || l.pts.size() != loops[0].pts.size()) segments = false;
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
    // SMLib sweeps the profile's bounding-box centre along the path (bMoveProfileCenterToPath),
    // not the profile frame origin. Equal for centred profiles; on a straight path an offset
    // is a plain translation; otherwise not representable.
    SmExtent3d box;
    for (SmCurve* c : cl.v) {
      SmExtent3d cb;
      c->CalculateBoundingBox(c->GetNaturalInterval(), &cb);
      box.AddPoint3d(cb.GetMin());
      box.AddPoint3d(cb.GetMax());
    }
    V3 off = fromsm(box.GetMid()) - pp[0];
    double psize = len(fromsm(box.GetMax()) - fromsm(box.GetMin()));
    bool centred = len(off) <= 1e-9 * std::max(1.0, psize);
    if (!centred && !straight)
      throw fail(SC_UNSUPPORTED,
                 "sweep of a profile that is not centred on its origin along a curved path");
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
      SC_CHECK(SmApiCreateCurveSweep(prof, pathc, nullptr, nullptr, &opt, r, nullptr, nullptr,
                                     nullptr),
               "SmApiCreateCurveSweep");
    }
    BrepPtr b(r);
    require_solid(r, "sweep");
    if (!centred) {
      M4 t = identity();
      t.m[12] = off.x;
      t.m[13] = off.y;
      t.m[14] = off.z;
      apply_matrix(b.get(), t);
    }
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
    SmSurface* s = nullptr;
    SC_CHECK(SmApiCreateSurfaceFromOrderedPoints(p, rows, cols, s),
             "SmApiCreateSurfaceFromOrderedPoints");
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
    SmBrep* r = nullptr;
    SC_CHECK(SmApiShellBrepFull(in, -t, TRUE, TRUE, FALSE, open, r), "SmApiShellBrepFull");
    require_result(r, "shell");
    BrepPtr b(r);
    check_valid(b.get(), "shell");
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
      meta.push_back((uint32_t)ep.size());
      meta.push_back((uint32_t)tags.size());
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
      if (SmApiBrepComputeVolume(b, 1e-8, fine) == SM_SUCCESS && is_fin(fine)) vol = fine;
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

}  // extern "C"
