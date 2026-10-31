// Studio look: the mirror image of the model in the ground (like `mcad render
// --ground-reflection`). The model is rendered once more by a camera mirrored at the ground
// plane into a render target (colour with mip levels, depth); the ground's shader looks the
// mirror image up projectively, blurs it by the height of the reflected point (mip level from
// the roughness), fades it with that height and blends it in with a Fresnel weight — the same
// weights as `cad_render::studio`. Nothing below the ground: no oblique clip plane needed.

import * as THREE from "three";

export class GroundReflection {
  private rt: THREE.WebGLRenderTarget | null = null;
  private cam = new THREE.PerspectiveCamera();
  /** Uniforms of the ground shader (shared object: values change per frame). */
  readonly uniforms = {
    tReflect: { value: null as THREE.Texture | null },
    tReflectDepth: { value: null as THREE.Texture | null },
    reflMatrix: { value: new THREE.Matrix4() },
    reflInvPV: { value: new THREE.Matrix4() },
    /** strength (0: off), roughness, ground z, fade height */
    reflParams: { value: new THREE.Vector4(0, 0.3, 0, 1) },
    /** render target height in pixels, tan(fov / 2) */
    reflView: { value: new THREE.Vector2(1, 1) },
  };

  /** Strength of the last frame (0: off). */
  get strength(): number {
    return this.uniforms.reflParams.value.x;
  }

  /** No mirror image this frame. */
  off(): void {
    this.uniforms.reflParams.value.x = 0;
  }

  /**
   * Renders what `scene` shows (the caller hides all but the model) as seen in the mirror of
   * the plane z = `z0` from `camera`.
   */
  render(r: THREE.WebGLRenderer, scene: THREE.Scene, camera: THREE.PerspectiveCamera, z0: number, strength: number, roughness: number, fadeHeight: number, w: number, h: number): void {
    if (!this.rt || this.rt.width !== w || this.rt.height !== h) {
      this.rt?.dispose();
      this.rt = new THREE.WebGLRenderTarget(w, h, {
        type: THREE.HalfFloatType,
        minFilter: THREE.LinearMipmapLinearFilter,
        magFilter: THREE.LinearFilter,
        generateMipmaps: true,
        depthTexture: new THREE.DepthTexture(w, h),
      });
    }
    camera.updateMatrixWorld();
    const mirror = (v: THREE.Vector3) => v.set(v.x, v.y, 2 * z0 - v.z);
    const eye = mirror(new THREE.Vector3().setFromMatrixPosition(camera.matrixWorld));
    const look = mirror(camera.getWorldDirection(new THREE.Vector3()).add(new THREE.Vector3().setFromMatrixPosition(camera.matrixWorld)));
    const up = new THREE.Vector3(0, 1, 0).applyQuaternion(camera.quaternion);
    const c = this.cam;
    c.copy(camera, false);
    c.position.copy(eye);
    c.up.set(up.x, up.y, -up.z);
    c.lookAt(look);
    c.updateMatrixWorld();
    c.projectionMatrix.copy(camera.projectionMatrix);
    c.projectionMatrixInverse.copy(camera.projectionMatrixInverse);
    const pv = new THREE.Matrix4().multiplyMatrices(c.projectionMatrix, c.matrixWorldInverse);
    const bias = new THREE.Matrix4().set(0.5, 0, 0, 0.5, 0, 0.5, 0, 0.5, 0, 0, 0.5, 0.5, 0, 0, 0, 1);
    const u = this.uniforms;
    u.reflMatrix.value.multiplyMatrices(bias, pv);
    u.reflInvPV.value.copy(pv).invert();
    u.reflParams.value.set(strength, roughness, z0, fadeHeight);
    u.reflView.value.set(h, Math.tan((camera.fov * Math.PI) / 360) / Math.max(1e-6, camera.zoom));
    u.tReflect.value = this.rt.texture;
    u.tReflectDepth.value = this.rt.depthTexture;
    const prevTarget = r.getRenderTarget();
    const clear = r.getClearColor(new THREE.Color());
    const alpha = r.getClearAlpha();
    r.setRenderTarget(this.rt);
    r.setClearColor(0x000000, 0);
    r.clear();
    r.render(scene, c);
    r.setRenderTarget(prevTarget);
    r.setClearColor(clear, alpha);
  }

  dispose(): void {
    this.rt?.depthTexture?.dispose();
    this.rt?.dispose();
    this.rt = null;
  }
}

/** Ground fragment shader: declarations (after `#include <common>`; needs `vGroundPos`, the
 *  world position). */
export const GROUND_REFLECTION_PARS = `
uniform sampler2D tReflect;
uniform sampler2D tReflectDepth;
uniform mat4 reflMatrix;
uniform mat4 reflInvPV;
uniform vec4 reflParams;
uniform vec2 reflView;
`;

/** Ground fragment shader: blends the mirror image in (before tone mapping, linear light). */
export const GROUND_REFLECTION_BLEND = `
	if ( reflParams.x > 0.0 ) {
		vec4 rc = reflMatrix * vec4( vGroundPos, 1.0 );
		vec2 ruv = rc.xy / rc.w;
		if ( ruv.x > 0.0 && ruv.y > 0.0 && ruv.x < 1.0 && ruv.y < 1.0 ) {
			float rd = texture2D( tReflectDepth, ruv ).r;
			if ( rd < 1.0 ) {
				vec4 rp = reflInvPV * vec4( ruv * 2.0 - 1.0, rd * 2.0 - 1.0, 1.0 );
				float rh = max( 0.0, rp.z / rp.w - reflParams.z );
				float rfade = 1.0 - smoothstep( 0.0, reflParams.w, rh );
				float rndv = clamp( normalize( cameraPosition - vGroundPos ).z, 0.0, 1.0 );
				float ralpha = pow2( reflParams.y * 0.5 );
				float rradius = 2.0 * ralpha * rh / max( rndv, 0.2 );
				float rpx = reflView.x / ( 2.0 * distance( cameraPosition, vGroundPos ) * reflView.y );
				float rlod = log2( max( 1.0, rradius * rpx ) );
				vec4 rcol = textureLod( tReflect, ruv, rlod );
				float rs = reflParams.x;
				float rf = rs + ( 1.0 - rs ) * pow( 1.0 - rndv, 5.0 ) * rs;
				gl_FragColor.rgb = gl_FragColor.rgb * ( 1.0 - rf * rfade * rcol.a ) + rcol.rgb * rf * rfade;
			}
		}
	}
`;
