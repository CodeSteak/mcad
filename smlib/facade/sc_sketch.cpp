// controller-cad facade: sketches (Plan 17.1) -- composite profile loops of exact segments
// (lines, circular arcs, B-splines) for extrude, revolve, loft and sweep, and sweeps along
// an open sketch path (sc_sketch_sweep).
//
// This file is part of the translation unit of sc_facade.cpp: it is included at its end and
// uses its internal helpers (Loop, CurveList, M4, fail, SC_CHECK, ...). Compiled on its own
// (the build compiles every facade/*.cpp) it is empty: SC_FACADE_INTERNAL is only defined
// by sc_facade.cpp.
//
// Loop encoding (kind SC_LOOP_COMPOSITE, see sc_facade.h): count = number of segments, then
// per segment
//   0 line   x0 y0 x1 y1
//   1 arc    cx cy r a0 a1            (radians; a1 > a0 counter-clockwise, a1 < a0 clockwise)
//   2 nurbs  degree n_ctrl n_knots knots... (x y w) per control point
// Consecutive segments must meet (end of one = start of the next, the last ends at the
// start of the first for closed loops); the facade welds the joints exactly.

#ifdef SC_FACADE_INTERNAL

namespace {

// Segment of a composite loop in its profile plane (local coordinates).
struct SkSeg {
  int type = 0;  // 0 line, 1 arc, 2 nurbs
  // line: a, b; arc: center, r, a0, a1
  double x0 = 0, y0 = 0, x1 = 0, y1 = 0;
  double cx = 0, cy = 0, r = 0, a0 = 0, a1 = 0;
  // B-spline (arcs are converted to one)
  uint32_t degree = 0;
  std::vector<double> knots;  // full knot vector
  std::vector<V3> pts;        // control points (z = 0)
  std::vector<double> w;      // weights
};

uint32_t sketch_parse(const double* d, uint32_t i, uint32_t dlen, uint32_t count, Loop& l) {
  if (count == 0) throw fail(SC_INVALID, "sketch loop without segments");
  for (uint32_t s = 0; s < count; s++) {
    if (i >= dlen) throw fail(SC_INVALID, "truncated sketch data");
    int t = (int)d[i];
    uint32_t n = 0;
    if (t == 0) {
      n = 5;
    } else if (t == 1) {
      n = 6;
    } else if (t == 2) {
      if (i + 4 > dlen) throw fail(SC_INVALID, "truncated sketch data");
      uint32_t deg = (uint32_t)d[i + 1], nc = (uint32_t)d[i + 2], nk = (uint32_t)d[i + 3];
      if (deg < 1 || deg > 9 || nc < deg + 1 || nk != nc + deg + 1 || nc > 100000)
        throw fail(SC_INVALID, "bad sketch B-spline");
      n = 4 + nk + 3 * nc;
    } else {
      throw fail(SC_INVALID, "unknown sketch segment type " + std::to_string(t));
    }
    if (i + n > dlen) throw fail(SC_INVALID, "truncated sketch data");
    for (uint32_t k = 0; k < n; k++) {
      if (!is_fin(d[i + k])) throw fail(SC_INVALID, "non-finite sketch data");
      l.sk.push_back(d[i + k]);
    }
    i += n;
  }
  if (i + 16 > dlen) throw fail(SC_INVALID, "truncated loop data");
  return i;
}

std::vector<SkSeg> sketch_segments(const Loop& l) {
  std::vector<SkSeg> out;
  const std::vector<double>& d = l.sk;
  size_t i = 0;
  while (i < d.size()) {
    SkSeg s;
    s.type = (int)d[i];
    if (s.type == 0) {
      s.x0 = d[i + 1];
      s.y0 = d[i + 2];
      s.x1 = d[i + 3];
      s.y1 = d[i + 4];
      i += 5;
    } else if (s.type == 1) {
      s.cx = d[i + 1];
      s.cy = d[i + 2];
      s.r = d[i + 3];
      s.a0 = d[i + 4];
      s.a1 = d[i + 5];
      i += 6;
      if (!(s.r > 0)) throw fail(SC_INVALID, "sketch arc radius must be > 0");
      double sweep = s.a1 - s.a0;
      if (std::fabs(sweep) < 1e-12 || std::fabs(sweep) > 2 * M_PI + 1e-9)
        throw fail(SC_INVALID, "sketch arc sweep must be in (0, 2pi]");
      // rational quadratic arc, pieces of at most 90 degrees (Piegl & Tiller A7.1)
      int np = std::max(1, (int)std::ceil(std::fabs(sweep) / (M_PI / 2 + 1e-9)));
      double dt = sweep / np, w1 = std::cos(dt / 2);
      s.type = 2;
      s.degree = 2;
      s.pts.push_back(v3(s.cx + s.r * std::cos(s.a0), s.cy + s.r * std::sin(s.a0), 0));
      s.w.push_back(1);
      s.knots = {0, 0, 0};
      for (int k = 0; k < np; k++) {
        double a = s.a0 + dt * k;
        double rm = s.r / w1;
        s.pts.push_back(v3(s.cx + rm * std::cos(a + dt / 2), s.cy + rm * std::sin(a + dt / 2), 0));
        s.w.push_back(w1);
        s.pts.push_back(v3(s.cx + s.r * std::cos(a + dt), s.cy + s.r * std::sin(a + dt), 0));
        s.w.push_back(1);
        if (k + 1 < np) {
          s.knots.push_back(k + 1);
          s.knots.push_back(k + 1);
        }
      }
      for (int k = 0; k < 3; k++) s.knots.push_back(np);
      s.type = 1;  // keep the arc marker (curve form), data is the B-spline
    } else {
      s.degree = (uint32_t)d[i + 1];
      uint32_t nc = (uint32_t)d[i + 2], nk = (uint32_t)d[i + 3];
      i += 4;
      s.knots.assign(d.begin() + i, d.begin() + i + nk);
      i += nk;
      for (uint32_t k = 0; k < nc; k++) {
        s.pts.push_back(v3(d[i], d[i + 1], 0));
        double w = d[i + 2];
        if (!(w > 0)) throw fail(SC_INVALID, "sketch B-spline weights must be > 0");
        s.w.push_back(w);
        i += 3;
      }
      for (size_t k = 1; k < s.knots.size(); k++)
        if (s.knots[k] < s.knots[k - 1]) throw fail(SC_INVALID, "sketch B-spline knots must not decrease");
    }
    out.push_back(std::move(s));
  }
  return out;
}

V3 seg_start(const SkSeg& s) { return s.type == 0 ? v3(s.x0, s.y0, 0) : s.pts.front(); }
V3 seg_end(const SkSeg& s) { return s.type == 0 ? v3(s.x1, s.y1, 0) : s.pts.back(); }

void set_start(SkSeg& s, V3 p) {
  if (s.type == 0) {
    s.x0 = p.x;
    s.y0 = p.y;
  } else {
    s.pts.front() = p;
  }
}

// Welds the joints: every segment starts exactly where the previous one ends (closed: the
// first starts where the last ends). Gaps above `gap` are an error.
void weld(std::vector<SkSeg>& segs, bool closed) {
  double ext = 1;
  for (const SkSeg& s : segs) ext = std::max({ext, len(seg_start(s)), len(seg_end(s))});
  double gap = 1e-6 * ext;
  for (size_t k = 0; k < segs.size(); k++) {
    if (k == 0 && !closed) continue;
    const SkSeg& prev = segs[(k + segs.size() - 1) % segs.size()];
    V3 e = seg_end(prev);
    if (len(e - seg_start(segs[k])) > gap)
      throw fail(SC_INVALID, "sketch segments do not meet (segment " + std::to_string(k) + ")");
    set_start(segs[k], e);
  }
}

// Distinct knots and multiplicities of a full knot vector.
void knot_mults(const std::vector<double>& kv, SmTArray<double>& knots, SmTArray<ULONG>& mult) {
  std::vector<double> ks;
  std::vector<ULONG> ms;
  for (size_t k = 0; k < kv.size(); k++) {
    if (k > 0 && kv[k] == kv[k - 1]) {
      ms.back()++;
    } else {
      ks.push_back(kv[k]);
      ms.push_back(1);
    }
  }
  for (double k : ks) knots.Add(k);
  for (ULONG m : ms) mult.Add(m);
}

SmCurve* make_curve(const SkSeg& s, const M4& f) {
  if (s.type == 0) {
    SmLine* ln = nullptr;
    SC_CHECK(SmApiCreateLineSegment(sp(f.point(v3(s.x0, s.y0, 0))), sp(f.point(v3(s.x1, s.y1, 0))), ln),
             "SmApiCreateLineSegment");
    return ln;
  }
  SmTArray<SmPoint3d> cp;
  for (V3 p : s.pts) cp.Add(sp(f.point(p)));
  SmTArray<double> knots;
  SmTArray<ULONG> mult;
  knot_mults(s.knots, knots, mult);
  bool rational = false;
  for (double w : s.w)
    if (std::fabs(w - s.w[0]) > 1e-14 * std::fabs(s.w[0])) rational = true;
  SmTArray<double> wts;
  for (double w : s.w) wts.Add(w);
  SmBSplineCurve* c = nullptr;
  SmBSplineCurveForm form = (s.type == 1) ? SM_CF_CIRCULAR_ARC : SM_CF_UNSPECIFIED;
  // an arc under a non-uniform frame is an ellipse arc
  double sc;
  if (s.type == 1 && !similarity(f, sc)) form = SM_CF_ELLIPTIC_ARC;
  SC_CHECK(SmApiCreateCanonicalCurve(cp, knots, mult, rational ? &wts : nullptr, s.degree, form, c),
           "SmApiCreateCanonicalCurve");
  return c;
}

// Curves of a composite loop in the space `f` maps the profile plane to (any affine map: the
// B-spline control points are mapped, which is exact).
void sketch_curves(const Loop& l, const M4& f, bool single, CurveList& out) {
  (void)single;  // sections of different segment counts are joined by the caller
  std::vector<SkSeg> segs = sketch_segments(l);
  weld(segs, true);
  for (const SkSeg& s : segs) out.add(make_curve(s, f));
}

// Loft: sections are matched segment by segment when all are polygons with the same number of
// points or all are composite loops with the same number of segments; otherwise every
// section is one closed curve.
bool loft_segmentwise(const std::vector<Loop>& loops) {
  bool poly = true, comp = true;
  size_t ncomp = 0;
  for (size_t k = 0; k < loops.size(); k++) {
    const Loop& l = loops[k];
    if (l.kind != SC_LOOP_POLYGON || l.pts.size() != loops[0].pts.size()) poly = false;
    if (l.kind != SC_LOOP_COMPOSITE) {
      comp = false;
    } else {
      size_t n = sketch_segments(l).size();
      if (k == 0) ncomp = n;
      if (n != ncomp) comp = false;
    }
  }
  return poly || comp;
}

}  // namespace

extern "C" {

uint32_t sc_sketch_sweep(const double* loop, uint32_t len_, const double* path, uint32_t plen,
                         int32_t mode, uint32_t tag) {
  SC_SHAPE_CALL({
    auto loops = parse_loops(loop, len_, 1);
    auto paths = parse_loops(path, plen, 1);
    const Loop& pl = paths[0];
    if (pl.kind != SC_LOOP_COMPOSITE) throw fail(SC_INVALID, "sweep path must be a composite loop");
    std::vector<SkSeg> psegs = sketch_segments(pl);
    weld(psegs, false);
    CurveList pcl;
    for (const SkSeg& s : psegs) pcl.add(make_curve(s, pl.frame));
    // start point and tangent of the path
    V3 p0 = pl.frame.point(seg_start(psegs[0]));
    V3 z;
    {
      SmCurve* c0 = pcl.v[0];
      SmExtent1d iv = c0->GetNaturalInterval();
      SmVector3d pt, d1;
      SmVector3d* ppt = &pt;
      SmVector3d* pd1 = &d1;
      SmVector3d* pd2 = nullptr;
      SC_CHECK(SmApiEvaluateCurve(c0, iv.GetMin(), ppt, pd1, pd2), "SmApiEvaluateCurve");
      z = normalize(v3(d1.x, d1.y, d1.z));
      if (len(z) == 0) throw fail(SC_INVALID, "sweep path starts with a zero tangent");
    }
    // profile placement as in sc_sweep: x = global X projected into the normal plane
    V3 ref = std::fabs(z.x) < 0.9 ? v3(1, 0, 0) : v3(0, 1, 0);
    V3 x = normalize(ref - z * dot(ref, z));
    V3 y = cross(z, x);
    Loop l = loops[0];
    l.frame = identity();
    l.frame.m[0] = x.x; l.frame.m[1] = x.y; l.frame.m[2] = x.z;
    l.frame.m[4] = y.x; l.frame.m[5] = y.y; l.frame.m[6] = y.z;
    l.frame.m[8] = z.x; l.frame.m[9] = z.y; l.frame.m[10] = z.z;
    l.frame.m[12] = p0.x; l.frame.m[13] = p0.y; l.frame.m[14] = p0.z;
    CurveList cl;
    loop_curves(l, false, false, cl);
    SmTArray<SmCurve*> prof = as_array(cl);
    // As in sc_sweep: the curve sweep moves the profile frame origin along the path
    // (bUseProfileRefPoint, smlib/patches/0003); the mitered planar-path sweep moves the
    // profile's box centre, so there the profile must be centred.
    SmExtent3d box;
    for (SmCurve* c : cl.v) {
      SmExtent3d cb;
      c->CalculateBoundingBox(c->GetNaturalInterval(), &cb);
      box.AddPoint3d(cb.GetMin());
      box.AddPoint3d(cb.GetMax());
    }
    double psize = len(fromsm(box.GetMax()) - fromsm(box.GetMin()));
    bool centred = len(fromsm(box.GetMid()) - p0) <= 1e-9 * std::max(1.0, psize);
    bool mitered = mode == 0 && pcl.v.size() > 1;
    if (!centred && mitered)
      throw fail(SC_UNSUPPORTED,
                 "sweep of a profile that is not centred on its origin along a path with corners");
    SmBrep* r = nullptr;
    if (mitered) {
      // planar path with corners: mitered joints
      SmTArray<SmCurve*> segarr = as_array(pcl);
      SC_CHECK(SmApiCreateSweepAlongPlanarPath(prof, segarr, TRUE, 3, r),
               "SmApiCreateSweepAlongPlanarPath");
    } else {
      if (pcl.v.size() != 1) throw fail(SC_INVALID, "smooth sweep path must be one curve");
      SmBSplineCurve* pathc = nullptr;
      ObjPtr<SmBSplineCurve> owner;
      if (pcl.v[0]->IsKindOf(SmBSplineCurve_TYPE)) {
        pathc = (SmBSplineCurve*)pcl.v[0];
      } else {
        pathc = polyline_curve({p0, pl.frame.point(seg_end(psegs[0]))}, false);
        owner.p = pathc;
      }
      SmSweepOptions opt;
      opt.bCapEndsArg = TRUE;
      opt.bMoveProfileCenterToPath = TRUE;
      opt.bUseProfileRefPoint = TRUE;
      opt.sProfileRefPoint = SmVector3d(p0.x, p0.y, p0.z);
      SC_CHECK(SmApiCreateCurveSweep(prof, pathc, nullptr, nullptr, &opt, r, nullptr, nullptr, nullptr),
               "SmApiCreateCurveSweep");
    }
    BrepPtr b(r);
    require_solid(r, "sweep");
    tag_all(b.get(), tag);
    _res = put(std::move(b));
  })
}

}  // extern "C"

#endif  // SC_FACADE_INTERNAL
