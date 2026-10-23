// Mesh cache by content key (`SceneNode.mesh_key`): engine meshes and the three.js geometries
// built from them. Hover and cursor changes reuse them without asking the engine again.

import * as THREE from "three";
import type { MeshData } from "../engine/protocol";

export interface CachedMesh {
  data: MeshData;
  bytes: number;
  geometry: THREE.BufferGeometry | null;
  edges: THREE.BufferGeometry | null;
}

const LIMIT_BYTES = 256 << 20;

function sizeOf(m: MeshData): number {
  return (
    m.positions.byteLength + m.normals.byteLength + m.indices.byteLength + m.tri_tags.byteLength + m.tri_faces.byteLength + m.edge_segments.byteLength + m.edge_ids.byteLength
  );
}

export class MeshCache {
  private map = new Map<string, CachedMesh>();
  private bytes = 0;

  has(key: string | null): boolean {
    return key !== null && this.map.has(key);
  }

  /** Stores a mesh; a fine mesh is not replaced by a coarse one. */
  put(m: MeshData): void {
    const old = this.map.get(m.key);
    if (old && !old.data.coarse && m.coarse) return;
    if (old) this.drop(m.key);
    const e: CachedMesh = { data: m, bytes: sizeOf(m), geometry: null, edges: null };
    this.map.set(m.key, e);
    this.bytes += e.bytes;
    this.evict();
  }

  get(key: string | null): CachedMesh | null {
    if (key === null) return null;
    const e = this.map.get(key);
    if (!e) return null;
    // LRU order: most recently used last.
    this.map.delete(key);
    this.map.set(key, e);
    return e;
  }

  isCoarse(key: string | null): boolean {
    return key !== null && (this.map.get(key)?.data.coarse ?? false);
  }

  geometry(key: string): THREE.BufferGeometry | null {
    const e = this.get(key);
    if (!e) return null;
    if (!e.geometry) {
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.BufferAttribute(e.data.positions, 3));
      if (e.data.normals.length === e.data.positions.length) g.setAttribute("normal", new THREE.BufferAttribute(e.data.normals, 3));
      g.setIndex(new THREE.BufferAttribute(e.data.indices, 1));
      if (e.data.normals.length !== e.data.positions.length) g.computeVertexNormals();
      g.computeBoundingSphere();
      g.computeBoundingBox();
      e.geometry = g;
    }
    return e.geometry;
  }

  edges(key: string): THREE.BufferGeometry | null {
    const e = this.get(key);
    if (!e) return null;
    if (!e.edges) {
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.BufferAttribute(e.data.edge_segments, 3));
      g.computeBoundingSphere();
      e.edges = g;
    }
    return e.edges;
  }

  size(): { count: number; bytes: number } {
    return { count: this.map.size, bytes: this.bytes };
  }

  private drop(key: string): void {
    const e = this.map.get(key);
    if (!e) return;
    e.geometry?.dispose();
    e.edges?.dispose();
    this.bytes -= e.bytes;
    this.map.delete(key);
  }

  private evict(): void {
    for (const k of this.map.keys()) {
      if (this.bytes <= LIMIT_BYTES || this.map.size <= 1) break;
      this.drop(k);
    }
  }
}
