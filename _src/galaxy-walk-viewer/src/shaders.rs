//! GLSL sources. Own implementations of a physically scaled sky, sun, clouds,
//! fog, material shading and the image pipeline.

pub const COMMON: &str = r#"
#version 330 core
const float PI = 3.14159265;
float hash12(vec2 p){ vec3 p3 = fract(vec3(p.xyx) * .1031); p3 += dot(p3, p3.yzx + 33.33); return fract((p3.x + p3.y) * p3.z); }
float hash13(vec3 p3){ p3 = fract(p3 * .1031); p3 += dot(p3, p3.zyx + 31.32); return fract((p3.x + p3.y) * p3.z); }
float vnoise(vec2 p){ vec2 i = floor(p); vec2 f = fract(p); vec2 u = f*f*(3.0-2.0*f);
  return mix(mix(hash12(i), hash12(i+vec2(1,0)), u.x), mix(hash12(i+vec2(0,1)), hash12(i+vec2(1,1)), u.x), u.y); }
float fbm(vec2 p, int oct){ float a = 0.5, s = 0.0; for(int i=0;i<8;i++){ if(i>=oct) break; s += a*vnoise(p); p = p*2.03 + vec2(17.1, 9.2); a *= 0.5; } return s; }
float lum(vec3 c){ return dot(c, vec3(0.2126, 0.7152, 0.0722)); }
"#;

/// Sky / atmosphere functions shared by the sky pass and the material reflections.
pub const SKY: &str = r#"
uniform vec3 uSunDir;
uniform vec3 uSunColor;     // linear colour * illuminance (lux)
uniform float uDay;         // 0 night .. 1 day
uniform float uSkyLum;      // zenith luminance scale (cd/m2)
uniform float uNightSky;    // night sky brightness factor
uniform float uMoonSize;
uniform float uCloudDensity, uCloudVariety, uCloudCirrus;
uniform int uContrails;
uniform vec2 uCloudOffset;
uniform int uHorizon;
uniform float uHorizonRot;
uniform int uWhiteBg;
uniform int uUseSkybox;
uniform sampler2D uSkybox;
uniform float uSkyboxScale;
uniform float uTime;

vec3 atmosphere(vec3 d){
  float sh = uSunDir.y;
  float up = max(d.y, 0.0);
  vec3 zenith = mix(vec3(0.004, 0.006, 0.014), vec3(0.16, 0.34, 0.78), uDay);
  vec3 horizon = mix(vec3(0.010, 0.012, 0.020), vec3(0.62, 0.74, 0.88), uDay);
  float sunset = clamp(1.0 - abs(sh) * 5.0, 0.0, 1.0) * smoothstep(-0.25, 0.05, sh);
  float toward = pow(max(dot(normalize(vec3(d.x, 0.0, d.z) + 1e-4), normalize(vec3(uSunDir.x, 0.0, uSunDir.z) + 1e-4)), 0.0), 2.0);
  horizon = mix(horizon, vec3(1.0, 0.45, 0.18), sunset * (0.35 + 0.65 * toward));
  zenith = mix(zenith, vec3(0.25, 0.28, 0.45), sunset * 0.4);
  vec3 c = mix(horizon, zenith, pow(up, 0.45));
  float mu = max(dot(d, uSunDir), 0.0);
  c += vec3(1.0, 0.8, 0.6) * (pow(mu, 12.0) * 0.6 + pow(mu, 400.0) * 2.0) * (0.3 + sunset) * smoothstep(-0.1, 0.05, sh);
  c *= uSkyLum;
  // stars + moon at night
  float night = 1.0 - smoothstep(-0.15, 0.05, sh);
  if (night > 0.0 && d.y > 0.0) {
    vec3 g = floor(d * 400.0);
    float st = step(0.9985, hash13(g)) * hash13(g + 7.0);
    c += vec3(st) * 0.6 * night * uNightSky;
    vec3 moonDir = normalize(vec3(-uSunDir.x, abs(uSunDir.y) * 0.8 + 0.25, -uSunDir.z));
    float md = dot(d, moonDir);
    float size = cos(0.012 * uMoonSize / 100.0);
    c += vec3(0.9, 0.92, 1.0) * smoothstep(size, size + 0.00015, md) * 25.0 * night * uNightSky;
    c += vec3(0.6, 0.65, 0.8) * pow(max(md, 0.0), 300.0) * 0.4 * night * uNightSky;
  }
  return c;
}

float horizonProfile(float a){
  // a: azimuth in radians
  float x = a / (2.0 * PI);
  if (uHorizon == 1) return 0.03 + 0.11 * fbm(vec2(x * 9.0, 1.3), 5);                    // mountains
  if (uHorizon == 2) return 0.018 + 0.014 * fbm(vec2(x * 220.0, 4.0), 3);                 // forest
  if (uHorizon == 3) { float b = floor(x * 260.0); return 0.006 + 0.012 * hash12(vec2(b, 3.0)) * step(0.25, hash12(vec2(b, 5.0))); } // town
  if (uHorizon == 4) { float b = floor(x * 180.0); return 0.01 + 0.07 * pow(hash12(vec2(b, 9.0)), 2.0) * step(0.2, hash12(vec2(b, 2.0))); } // urban
  if (uHorizon == 5) return 0.004 + 0.02 * fbm(vec2(x * 14.0, 7.0), 3);                   // desert
  if (uHorizon == 6) { float b = floor(x * 120.0); float crane = step(0.985, hash12(vec2(b, 1.0))) * 0.09; return max(0.008 * step(0.4, hash12(vec2(b, 8.0))), crane); } // site
  if (uHorizon == 7) { float b = floor(x * 90.0); return 0.03 * step(0.45, hash12(vec2(b, 4.0))) * (0.5 + hash12(vec2(b, 6.0))); } // white cubes
  return -1.0;
}

vec3 horizonColor(float a){
  if (uHorizon == 1) return vec3(0.33, 0.38, 0.44);
  if (uHorizon == 2) return vec3(0.08, 0.16, 0.06);
  if (uHorizon == 3) return vec3(0.45, 0.40, 0.36);
  if (uHorizon == 4) return vec3(0.40, 0.42, 0.46);
  if (uHorizon == 5) return vec3(0.75, 0.60, 0.40);
  if (uHorizon == 6) return vec3(0.50, 0.48, 0.42);
  return vec3(0.9);
}

vec3 clouds(vec3 d, vec3 base){
  if (d.y <= 0.01) return base;
  vec2 p = d.xz / d.y * 1.2 + uCloudOffset;
  int oct = 3 + int(uCloudVariety / 25.0);
  float n = fbm(p * 0.6, oct);
  float cover = uCloudDensity / 100.0;
  float c = smoothstep(1.0 - cover * 0.7, 1.15 - cover * 0.5, n + cover * 0.12);
  float cirrus = fbm(vec2(p.x * 0.25, p.y * 2.5) + 3.0, 4);
  c = max(c, smoothstep(0.55, 0.9, cirrus) * uCloudCirrus / 100.0 * 0.6);
  for (int i = 0; i < 30; i++) {
    if (i >= uContrails) break;
    float ang = hash12(vec2(float(i), 1.0)) * PI;
    vec2 dir = vec2(cos(ang), sin(ang));
    float off = (hash12(vec2(float(i), 2.0)) - 0.5) * 6.0;
    float dist = abs(dot(p - uCloudOffset, vec2(-dir.y, dir.x)) - off);
    c = max(c, (1.0 - smoothstep(0.0, 0.03, dist)) * 0.7);
  }
  c *= smoothstep(0.01, 0.12, d.y);
  float sunLit = 0.6 + 0.4 * max(dot(d, uSunDir), 0.0);
  vec3 cloudCol = (uSunColor / 100000.0 * 2.2 * sunLit + vec3(0.40, 0.43, 0.48) * uDay) * uSkyLum * 1.2 + vec3(0.002);
  return mix(base, cloudCol, c);
}

vec3 skyRadiance(vec3 d, bool withClouds){
  if (uWhiteBg == 1) return vec3(max(uSkyLum, 0.05) * 1.4);
  if (uUseSkybox == 1) {
    vec2 uv = vec2(atan(d.x, -d.z) / (2.0 * PI) + 0.5 + uHorizonRot / 360.0, acos(clamp(d.y, -1.0, 1.0)) / PI);
    return texture(uSkybox, uv).rgb * uSkyboxScale;
  }
  vec3 c = atmosphere(d);
  if (withClouds) c = clouds(d, c);
  float a = atan(d.x, -d.z) + radians(uHorizonRot);
  if (uHorizon == 8 && d.y < 0.0) return vec3(0.9) * uSkyLum * 2.0;
  float h = horizonProfile(a);
  if (h > -0.5 && asin(clamp(d.y, -1.0, 1.0)) < h) {
    vec3 hc = horizonColor(a) * (uSunColor / 100000.0 * 0.25 + vec3(0.12) * uDay + vec3(0.002));
    float haze = smoothstep(0.0, 0.12, h - asin(clamp(d.y, -1.0, 1.0)));
    c = mix(atmosphere(normalize(vec3(d.x, 0.0, d.z) + 1e-4)), hc * uSkyLum * 6.0, 0.35 + 0.5 * haze);
  }
  if (d.y < 0.0 && h < -0.5) c = mix(c, vec3(0.08, 0.09, 0.08) * uSkyLum * 2.0, smoothstep(0.0, -0.05, d.y));
  return c;
}
"#;

pub const FOG: &str = r#"
uniform float uFogDensity;
uniform float uFogHeight;
uniform vec3 uCamPos;
vec3 applyFog(vec3 col, vec3 worldPos, vec3 skyHorizon){
  vec3 v = worldPos - uCamPos;
  float dist = length(v);
  if (uFogDensity <= 0.0) return col;
  float h0 = uCamPos.y, h1 = worldPos.y;
  float k = 1.0 / max(uFogHeight, 1.0);
  float avg = (abs(h1 - h0) < 0.01) ? exp(-max(h0, 0.0) * k) : (exp(-max(h0, 0.0) * k) - exp(-max(h1, 0.0) * k)) / (k * (h1 - h0) + 1e-6);
  avg = clamp(abs(avg), 0.0, 1.0);
  float f = 1.0 - exp(-dist * uFogDensity * avg);
  vec3 dir = v / max(dist, 1e-3);
  float mie = pow(max(dot(dir, uSunDir), 0.0), 8.0);
  vec3 fogCol = skyHorizon + uSunColor / 100000.0 * uSkyLum * mie * 1.5;
  return mix(col, fogCol, clamp(f, 0.0, 1.0));
}
"#;

pub const SKY_VS: &str = r#"
out vec2 vNdc;
void main(){ vec2 p = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2); vNdc = p * 2.0 - 1.0; gl_Position = vec4(vNdc, 1.0, 1.0); }
"#;

pub const SKY_FS: &str = r#"
in vec2 vNdc;
uniform mat4 uInvViewProj;
uniform int uOutput;
layout(location=0) out vec4 oColor;
void main(){
  vec4 a = uInvViewProj * vec4(vNdc, -1.0, 1.0);
  vec4 b = uInvViewProj * vec4(vNdc, 1.0, 1.0);
  vec3 d = normalize(b.xyz / b.w - a.xyz / a.w);
  vec3 c = skyRadiance(d, true);
  // a thin veil of fog towards the horizon
  float fogAmt = clamp(uFogDensity * 60.0, 0.0, 0.9) * (1.0 - smoothstep(0.0, 0.25, d.y));
  c = mix(c, atmosphere(normalize(vec3(d.x, 0.02, d.z))), fogAmt * 0.6);
  if (uOutput == 1) { oColor = vec4(1.0, 1.0, 1.0, 0.0); return; }
  if (uOutput == 2 || uOutput == 3) { oColor = vec4(0.0, 0.0, 0.0, 0.0); return; }
  if (uOutput == 4) { oColor = vec4(0.0, 0.0, 0.0, 0.0); return; }
  oColor = vec4(c, 0.0);
}
"#;

pub const SHADOW_VS: &str = r#"
layout(location=0) in vec3 aPos;
uniform mat4 uModel;
uniform mat4 uLightVP;
void main(){ gl_Position = uLightVP * uModel * vec4(aPos, 1.0); }
"#;

pub const SHADOW_FS: &str = r#"
void main(){ }
"#;

pub const SCENE_VS: &str = r#"
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNor;
layout(location=2) in vec2 aUv;
uniform mat4 uModel;
uniform mat4 uViewProj;
uniform mat4 uLightVP;
uniform int uMatType;
uniform vec2 uWind;
uniform float uTimeV;
out vec3 vWorld;
out vec3 vNormal;
out vec2 vUv;
out vec4 vLight;
void main(){
  vec4 w = uModel * vec4(aPos, 1.0);
  if (uMatType == 3) {
    float h = max(w.y, 0.0);
    w.xz += uWind * (sin(uTimeV * 1.7 + w.x * 0.6 + w.z * 0.4) * 0.5 + 0.5) * 0.015 * h;
  }
  vWorld = w.xyz;
  vNormal = mat3(uModel) * aNor;
  vUv = aUv;
  vLight = uLightVP * w;
  gl_Position = uViewProj * w;
}
"#;

pub const SCENE_FS: &str = r#"
in vec3 vWorld;
in vec3 vNormal;
in vec2 vUv;
in vec4 vLight;
layout(location=0) out vec4 oColor;

uniform int uMatType;     // 0 generic 1 grass 2 water 3 foliage 4 clearcoat 5 carpet 6 self-illuminated
uniform vec3 uAlbedo;
uniform vec3 uTint;
uniform int uHasTex;
uniform sampler2D uTex;
uniform float uFade;
uniform vec2 uTexScale;
uniform float uTexRot;
uniform float uBump;
uniform float uRough;
uniform float uMetal;
uniform float uSpec;
uniform float uOpacity;
uniform float uIor;
uniform vec3 uGlassTint;
uniform int uFrosted;
uniform vec3 uEmissive;
uniform float uLuminance;
uniform float uWaveH, uWaveS, uCaustics;
uniform float uGrassH;
uniform int uGrassOn;
uniform int uDoubleSided;
uniform int uStyle;       // 0 none 1 white 2 polystyrol 3 light view
uniform float uTransmission;
uniform float uAmbient;
uniform float uLightScale;
uniform sampler2DShadow uShadow;
uniform float uShadowSoft;
uniform int uShadowTaps;
uniform int uNumLights;
uniform vec3 uLightPos[16];
uniform vec3 uLightCol[16];
uniform int uOutput;      // 0 beauty 1 depth 2 material id 3 object id
uniform float uDepthRange;
uniform vec3 uIdColor;
uniform vec3 uMatIdColor;
uniform vec2 uWindDir;
uniform float uTimeV;

uniform mat4 uCascade0;
uniform mat4 uCascade1;
uniform vec2 uCascadeTexel;   // world size of one shadow texel per cascade
uniform mat4 uSkyView;        // top-down view of the model (third atlas slot)
uniform float uSkyTexel;

// How much open sky is above this point: ~1 outdoors, low under roofs, slabs and canopies.
float skyVisibility(vec3 n){
  vec3 wp = vWorld + n * 0.35 + vec3(0.0, 0.05, 0.0);
  vec4 c = uSkyView * vec4(wp, 1.0);
  vec3 p = c.xyz / c.w * 0.5 + 0.5;
  if (p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0) return 1.0;
  float sz = float(textureSize(uShadow, 0).y);
  float stepUv = 0.6 / max(uSkyTexel * sz, 1e-3);   // ~0.6 m between taps
  float sum = 0.0;
  for (int x = -2; x <= 2; x++) for (int y = -2; y <= 2; y++) {
    vec2 q = clamp(p.xy + vec2(x, y) * stepUv, vec2(0.0), vec2(1.0));
    sum += texture(uShadow, vec3((q.x + 2.0) / 3.0, q.y, p.z - 0.0005));
  }
  return sum / 25.0;
}

// One cascade of the side-by-side atlas; returns -1 when the point is outside it.
float cascadeShadow(mat4 m, int ci, vec3 n, float ndl, float texelW){
  // normal offset (scaled by slope) instead of a large depth bias: no acne, no floating shadows
  float slope = sqrt(max(1.0 - ndl * ndl, 0.0));
  vec3 wp = vWorld + n * texelW * (1.0 + 2.0 * slope);
  vec4 c = m * vec4(wp, 1.0);
  vec3 p = c.xyz / c.w * 0.5 + 0.5;
  float edge = 2.0 / float(textureSize(uShadow, 0).y);
  if (p.x < edge || p.x > 1.0 - edge || p.y < edge || p.y > 1.0 - edge || p.z > 1.0) return -1.0;
  vec2 atlasTexel = 1.0 / vec2(textureSize(uShadow, 0));
  float sum = 0.0, cnt = 0.0;
  int r = uShadowTaps;
  for (int x = -4; x <= 4; x++) for (int y = -4; y <= 4; y++) {
    if (abs(x) > r || abs(y) > r) continue;
    vec2 q = p.xy + vec2(x, y) * atlasTexel * vec2(3.0, 1.0) * uShadowSoft;
    q = clamp(q, vec2(0.0), vec2(1.0));
    sum += texture(uShadow, vec3((q.x + float(ci)) / 3.0, q.y, p.z - 0.00005));
    cnt += 1.0;
  }
  return sum / max(cnt, 1.0);
}

float shadowAt(vec3 n){
  float ndl = clamp(dot(n, uSunDir), 0.0, 1.0);
  float sNear = cascadeShadow(uCascade0, 0, n, ndl, uCascadeTexel.x);
  float sFar = cascadeShadow(uCascade1, 1, n, ndl, uCascadeTexel.y);
  if (sNear < 0.0) return sFar < 0.0 ? 1.0 : sFar;
  // fade into the far cascade near the edge of the sharp one
  vec4 c = uCascade0 * vec4(vWorld, 1.0);
  vec2 p = abs(c.xy / c.w);
  float k = smoothstep(0.8, 0.97, max(p.x, p.y));
  return sFar < 0.0 ? sNear : mix(sNear, sFar, k);
}

float D_GGX(float nh, float a){ float a2 = a*a; float d = nh*nh*(a2-1.0)+1.0; return a2 / (PI*d*d + 1e-6); }
float V_Smith(float nv, float nl, float a){ float k = a * 0.5; return 0.25 / ((nv*(1.0-k)+k)*(nl*(1.0-k)+k) + 1e-5); }
vec3 F_Schlick(float vh, vec3 f0){ return f0 + (1.0 - f0) * pow(1.0 - vh, 5.0); }

void main(){
  vec3 n = normalize(vNormal);
  vec3 v = normalize(uCamPos - vWorld);
  if (!gl_FrontFacing) n = -n;

  if (uOutput == 1) { float d = length(uCamPos - vWorld) / max(uDepthRange, 0.01); oColor = vec4(vec3(clamp(d, 0.0, 1.0)), 1.0); return; }
  if (uOutput == 2) { oColor = vec4(uMatIdColor, 1.0); return; }
  if (uOutput == 3) { oColor = vec4(uIdColor, 1.0); return; }

  // ---- albedo
  vec2 uv = vUv;
  float cr = cos(radians(uTexRot)), sr = sin(radians(uTexRot));
  uv = mat2(cr, -sr, sr, cr) * uv / max(uTexScale, vec2(0.001));
  // material colours are picked in sRGB; light them in linear space
  vec3 baseLin = pow(max(uAlbedo, vec3(0.0)), vec3(2.2));
  vec3 albedo = baseLin;
  float texAlpha = 1.0;
  if (uHasTex == 1) {
    vec4 t = texture(uTex, uv);
    texAlpha = t.a;
    albedo = mix(baseLin, pow(t.rgb, vec3(2.2)) * uTint, uFade);
  } else {
    albedo *= uTint;
  }
  float rough = clamp(uRough, 0.03, 1.0);
  float metal = uMetal;
  float opacity = uOpacity;

  // ---- procedural detail / bump
  if (abs(uBump) > 0.001) {
    float e = 0.02;
    vec2 bp = uv * 8.0;
    float h0 = fbm(bp, 4), hx = fbm(bp + vec2(e, 0), 4), hy = fbm(bp + vec2(0, e), 4);
    vec3 t = normalize(cross(n, abs(n.y) < 0.9 ? vec3(0,1,0) : vec3(1,0,0)));
    vec3 b = cross(n, t);
    n = normalize(n + (t * (h0 - hx) + b * (h0 - hy)) / e * uBump * 0.04);
    albedo *= 1.0 - 0.15 * abs(uBump) * (h0 - 0.5);
  }
  if (uMatType == 1) {
    float var = fbm(vWorld.xz * 0.15, 4);
    albedo *= 0.75 + 0.5 * var;
    if (uGrassOn == 1) {
      float blades = vnoise(vWorld.xz * 60.0) * vnoise(vWorld.xz * 23.0 + 3.0);
      float dist = length(uCamPos - vWorld);
      float fade = 1.0 - smoothstep(8.0, 40.0, dist);
      albedo *= mix(1.0, 0.55 + 0.9 * blades, fade * clamp(uGrassH * 8.0, 0.2, 1.0));
      n = normalize(n + vec3(vnoise(vWorld.xz * 40.0) - 0.5, 0.0, vnoise(vWorld.zx * 40.0) - 0.5) * 0.6 * fade);
    }
  }
  if (uMatType == 5) {
    albedo *= 0.85 + 0.3 * vnoise(vWorld.xz * 90.0);
  }
  if (uMatType == 2) {
    vec2 wd = uWindDir;
    float s = mix(0.4, 3.0, uWaveS);
    float t = uTimeV;
    vec2 p = vWorld.xz * s;
    float hx = 0.0, hz = 0.0;
    for (int i = 0; i < 4; i++) {
      float fi = float(i);
      vec2 dir = normalize(vec2(cos(fi * 1.7 + 0.3), sin(fi * 1.7 + 0.3)) + wd * 0.5);
      float fr = 1.5 + fi * 1.3;
      float ph = dot(dir, p) * fr + t * (1.2 + fi * 0.4);
      hx += dir.x * cos(ph) * fr; hz += dir.y * cos(ph) * fr;
    }
    n = normalize(vec3(-hx * uWaveH * 0.03, 1.0, -hz * uWaveH * 0.03));
    rough = 0.03;
  }

  // ---- rendering styles
  if (uStyle == 1) { albedo = vec3(0.82); metal = 0.0; rough = max(rough, 0.6); if (opacity < 0.999) opacity = max(opacity, 0.35); }
  if (uStyle == 2) { albedo = vec3(0.86); metal = 0.0; rough = 0.7; }
  if (uMatType == 6 && uStyle == 0) { }

  // ---- lighting (radiance in cd/m2, illuminance in lux)
  float ndl = dot(n, uSunDir);
  float wrap = 0.0;
  if (uMatType == 3) wrap = 0.35;
  if (uStyle == 2) wrap = uTransmission * 0.6;
  float diffN = max((ndl + wrap) / (1.0 + wrap), 0.0);
  float sh = (uSunDir.y > -0.05) ? shadowAt(n) : 0.0;
  if (uStyle == 2) sh = mix(sh, 1.0, uTransmission * 0.5);
  vec3 sunE = uSunColor * sh;
  // sky irradiance: zenith plus the brighter band near the horizon, seen from the side of n
  vec3 nhz = normalize(vec3(n.x, 0.0, n.z) + vec3(1e-4, 0.0, 0.0));
  vec3 skyUp = (atmosphere(vec3(0.0, 1.0, 0.0)) * 0.45 + atmosphere(normalize(nhz + vec3(0.0, 0.25, 0.0))) * 0.55) * PI;
  // the sky is brighter on the side of the sun
  skyUp *= 1.0 + 0.35 * max(dot(nhz, normalize(vec3(uSunDir.x, 0.0, uSunDir.z) + 1e-4)), 0.0) * uDay;
  // light bounced off the ground (albedo ~0.2) from sun + sky
  vec3 groundE = vec3(0.21, 0.20, 0.17) * (uSunColor * max(uSunDir.y, 0.0) + vec3(lum(skyUp)));
  // the sky dome is far less saturated in irradiance than the zenith colour suggests
  skyUp = mix(vec3(lum(skyUp)), skyUp, 0.6);
  float up = n.y * 0.5 + 0.5;
  // indoors and under overhangs most of the sky is hidden; keep some light coming in through openings
  float skyVis = (uOutput == 0) ? skyVisibility(n) : 1.0;
  float open = mix(0.12, 1.0, skyVis);
  vec3 ambE = (skyUp * up + groundE * (1.0 - up)) * uAmbient * open;
  ambE *= (uStyle == 1 || uStyle == 2) ? 1.1 : 1.0;

  vec3 lightsE = vec3(0.0);
  vec3 lightsSpec = vec3(0.0);
  for (int i = 0; i < 16; i++) {
    if (i >= uNumLights) break;
    vec3 L = uLightPos[i] - vWorld;
    float d2 = max(dot(L, L), 0.04);
    vec3 l = L * inversesqrt(d2);
    float att = 1.0 / d2 * pow(clamp(1.0 - d2 / 900.0, 0.0, 1.0), 2.0);
    float nl = max(dot(n, l), 0.0);
    vec3 E = uLightCol[i] * att * uLightScale;
    lightsE += E * nl;
    vec3 hh = normalize(l + v);
    lightsSpec += E * nl * D_GGX(max(dot(n, hh), 0.0), rough * rough) * 0.25;
  }

  vec3 E_total = sunE * max(ndl, 0.0) + ambE + lightsE;
  if (uStyle == 3) { oColor = vec4(vec3(lum(E_total)), 1.0); return; }

  vec3 f0 = mix(vec3(0.04 * uSpec * 2.0), albedo, metal);
  float nv = max(dot(n, v), 1e-3);
  vec3 h = normalize(uSunDir + v);
  float nh = max(dot(n, h), 0.0);
  float a = rough * rough;
  vec3 F = F_Schlick(max(dot(v, h), 0.0), f0);
  vec3 spec = D_GGX(nh, a) * V_Smith(nv, max(ndl, 0.0), a) * F * sunE * max(ndl, 0.0);
  vec3 kd = (1.0 - F) * (1.0 - metal);
  vec3 diffuse = albedo / PI * (sunE * diffN + ambE + lightsE) * kd;

  // environment reflection
  vec3 r = reflect(-v, n);
  vec3 Fenv = f0 + (max(vec3(1.0 - rough), f0) - f0) * pow(1.0 - nv, 5.0);
  vec3 env = skyRadiance(normalize(vec3(r.x, max(r.y, -0.2), r.z)), false);
  if (r.y < 0.0) env = mix(env, vec3(lum(groundE)) / PI, smoothstep(0.0, -0.2, r.y));
  float envOcc = mix(0.35, 1.0, clamp(r.y * 2.0 + 0.5, 0.0, 1.0));
  vec3 refl = env * Fenv * pow(1.0 - rough, 2.0) * envOcc;

  vec3 col = diffuse + spec + refl + lightsSpec * F;

  if (uMatType == 3) {
    float back = max(dot(-n, uSunDir), 0.0) * pow(max(dot(-v, uSunDir), 0.0), 2.0);
    col += albedo * uSunColor * sh * back * 0.15 / PI;
  }
  if (uMatType == 4) {
    vec3 hc = normalize(uSunDir + v);
    col += vec3(D_GGX(max(dot(n, hc), 0.0), 0.003) * 0.05) * sunE * max(ndl, 0.0) * 0.25;
    col += env * (0.04 + 0.96 * pow(1.0 - nv, 5.0)) * 0.6;
  }
  if (uMatType == 5) col *= mix(0.8, 1.15, pow(1.0 - nv, 2.0));
  if (uMatType == 6 && uStyle != 1 && uStyle != 2) col += uEmissive * uLuminance;

  float alpha = 1.0;
  if (uMatType == 2) {
    float fres = 0.02 + 0.98 * pow(1.0 - nv, 5.0);
    vec3 body = albedo * (lum(uSunColor) * max(uSunDir.y, 0.0) * 0.35 + lum(ambE)) / PI * 0.6;
    float sparkle = pow(max(dot(reflect(-uSunDir, n), v), 0.0), 600.0) * uCaustics * 4.0;
    col = mix(body, env, fres) + uSunColor * sh * sparkle * 0.002;
    alpha = clamp(0.55 + fres * 0.45, 0.0, 1.0) * max(opacity, 0.4);
  } else if (opacity < 0.999) {
    float fres = (pow((1.0 - uIor) / (1.0 + uIor), 2.0));
    fres = fres + (1.0 - fres) * pow(1.0 - nv, 5.0);
    vec3 glassRefl = env * fres + spec;
    vec3 tinted = albedo * uGlassTint * (ambE * 0.2 + sunE * max(ndl, 0.0) * 0.1) / PI;
    if (uFrosted == 1) tinted += vec3(lum(ambE)) * 0.25 / PI;
    alpha = clamp(opacity + fres, 0.0, 1.0);
    col = (glassRefl + tinted * opacity) / max(alpha, 0.05);
  }
  if (uHasTex == 1 && texAlpha < 0.5) discard;

  col = applyFog(col, vWorld, atmosphere(normalize(vec3(vWorld.x - uCamPos.x, 0.03, vWorld.z - uCamPos.z) + 1e-4)));
  oColor = vec4(col, alpha);
}
"#;

pub const POST_FS: &str = r#"
in vec2 vNdc;
layout(location=0) out vec4 oColor;
uniform sampler2D uHdr;
uniform sampler2D uDepth;
uniform float uMaxLod;
uniform float uExposure;      // manual multiplier
uniform int uAutoExposure;
uniform float uBrightness;    // EV offset from the exposure slider
uniform float uHighlights, uShadows, uSaturation;
uniform vec3 uWhite;
uniform int uAutoContrast;
uniform float uBloom, uVignette, uChromatic, uFlare;
uniform float uDof, uFocal, uNear, uFar;
uniform int uOrtho;
uniform vec3 uSunScreen;      // xy in uv, z visible 0/1
uniform float uOutline;
uniform int uStyle;
uniform int uLightAuto;
uniform float uLvMin, uLvMax;
uniform int uRawOutput;       // depth / id passes skip the image pipeline
uniform int uFlipY;
uniform int uKeepAlpha;
uniform float uAo;            // ambient occlusion strength 0..1
uniform float uProjY;         // proj[1][1], for metres -> pixels
uniform float uProjX;         // proj[0][0]

float linDepth(float z){
  if (uOrtho == 1) return mix(uNear, uFar, z);
  float n = uNear, f = uFar;
  return 2.0 * n * f / (f + n - (z * 2.0 - 1.0) * (f - n));
}

// View-space position of a pixel from its linear depth.
vec3 viewPos(vec2 uv, float ld){ return vec3((uv * 2.0 - 1.0) * ld / vec2(uProjX, uProjY), -ld); }

// Screen-space ambient occlusion: darkens creases, corners and contact areas the sky cannot
// reach. Samples are tested against the surface normal rebuilt from depth, so flat walls seen
// at a grazing angle stay clean. Radius ~0.7 m in the world.
float ssao(vec2 uv, float ld){
  if (uAo <= 0.0 || uOrtho == 1 || ld > 250.0) return 1.0;
  vec2 res = vec2(textureSize(uDepth, 0));
  vec3 P = viewPos(uv, ld);
  vec3 N = normalize(cross(dFdx(P), dFdy(P)));
  if (dot(N, P) > 0.0) N = -N;
  float worldR = 0.7;
  float pxR = clamp(worldR * uProjY * 0.5 * res.y / max(ld, 0.05), 3.0, 120.0);
  // 4x4 interleaved rotation: a regular pattern instead of white noise
  vec2 ip = mod(floor(uv * res), 4.0);
  float ang0 = (ip.x * 4.0 + ip.y) * (6.2831853 / 16.0) + fract(ip.y * 0.37) * 0.4;
  float occ = 0.0;
  const int NS = 16;
  for (int i = 0; i < NS; i++) {
    float t = (float(i) + 0.5) / float(NS);
    float a = ang0 + float(i) * 2.399963;
    vec2 suv = uv + vec2(cos(a), sin(a)) * t * pxR / res;
    vec3 S = viewPos(suv, linDepth(texture(uDepth, suv).r));
    vec3 v = S - P;
    float d2 = dot(v, v);
    float fall = max(1.0 - d2 / (worldR * worldR * 4.0), 0.0);
    occ += max(dot(v, N) * inversesqrt(d2 + 1e-6) - 0.15, 0.0) * fall;
  }
  float ao = clamp(1.0 - occ / float(NS) * 1.8, 0.0, 1.0);
  return mix(1.0, ao, uAo);
}

vec3 aces(vec3 x){ return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0); }

vec3 falseColor(float t){
  t = clamp(t, 0.0, 1.0);
  vec3 c = mix(vec3(0.05, 0.05, 0.6), vec3(0.0, 0.6, 1.0), smoothstep(0.0, 0.25, t));
  c = mix(c, vec3(0.1, 0.85, 0.2), smoothstep(0.25, 0.5, t));
  c = mix(c, vec3(1.0, 0.9, 0.1), smoothstep(0.5, 0.75, t));
  c = mix(c, vec3(0.9, 0.1, 0.05), smoothstep(0.75, 1.0, t));
  return c;
}

void main(){
  vec2 uv = vNdc * 0.5 + 0.5;
  if (uFlipY == 1) uv.y = 1.0 - uv.y;
  vec4 src = texture(uHdr, uv);
  if (uRawOutput == 1) { oColor = vec4(src.rgb, uKeepAlpha == 1 ? src.a : 1.0); return; }

  if (uStyle == 3) {
    float lx = src.r;
    float lo = uLvMin, hi = uLvMax;
    if (uLightAuto == 1) { float avg = textureLod(uHdr, vec2(0.5), uMaxLod).r; lo = max(avg * 0.05, 0.1); hi = max(avg * 4.0, lo * 2.0); }
    float t = (log(max(lx, 1e-3)) - log(lo)) / (log(hi) - log(lo));
    vec3 c = src.a > 0.5 ? falseColor(t) : vec3(0.08);
    oColor = vec4(c, 1.0);
    return;
  }

  float depth = texture(uDepth, uv).r;
  float ld = linDepth(depth);
  vec3 c = src.rgb;
  if (depth < 1.0) c *= ssao(uv, ld);

  // depth of field
  if (uDof > 0.0) {
    float coc = abs(ld - uFocal) / max(ld, 0.01) * uDof * 6.0;
    coc = clamp(coc, 0.0, 4.0);
    c = textureLod(uHdr, uv, coc).rgb;
  }
  // chromatic aberration
  if (uChromatic > 0.0) {
    vec2 off = (uv - 0.5) * uChromatic * 0.012;
    c.r = mix(c.r, texture(uHdr, uv + off).r, 0.9);
    c.b = mix(c.b, texture(uHdr, uv - off).b, 0.9);
  }
  // exposure
  float avgL = lum(textureLod(uHdr, vec2(0.5), uMaxLod).rgb);
  // auto exposure, held inside a camera-like range (EV 7..15.5) so a bright sky cannot blow out the model
  float exposure = uAutoExposure == 1 ? clamp(0.14 / max(avgL, 1e-4), 1.0 / (1.2 * 46341.0), 1.0 / (1.2 * 128.0)) : uExposure;
  exposure *= exp2(uBrightness);
  // bloom
  if (uBloom > 0.0) {
    vec3 b = textureLod(uHdr, uv, 3.0).rgb * 0.5 + textureLod(uHdr, uv, 5.0).rgb * 0.35 + textureLod(uHdr, uv, 7.0).rgb * 0.15;
    vec3 bright = max(b * exposure - 0.8, 0.0);
    c += bright / exposure * uBloom * 1.5;
  }
  vec3 x = c * exposure;
  // lens flare / glare from the sun
  if (uFlare > 0.0 && uSunScreen.z > 0.5) {
    vec2 sp = uSunScreen.xy;
    vec2 dir = vec2(0.5) - sp;
    float glare = pow(max(1.0 - length(uv - sp) * 1.6, 0.0), 6.0);
    x += vec3(1.0, 0.9, 0.75) * glare * uFlare * 0.6;
    for (int i = 1; i <= 4; i++) {
      vec2 g = sp + dir * float(i) * 0.55;
      float r = 0.02 + 0.015 * float(i);
      float ghost = smoothstep(r, r * 0.6, length(uv - g));
      x += vec3(0.4, 0.6, 1.0) * ghost * uFlare * 0.06 / float(i);
    }
  }
  // white balance / colour temperature
  x *= uWhite;
  // tone mapping
  vec3 t = aces(x);
  // highlights / shadows
  float L = lum(t);
  t += t * uShadows * 0.6 * pow(1.0 - L, 3.0);
  t += t * uHighlights * 0.4 * pow(L, 2.0);
  if (uAutoContrast == 1) { t = clamp((t - 0.03) / 0.94, 0.0, 1.0); t = mix(t, t * t * (3.0 - 2.0 * t), 0.35); }
  // saturation
  L = lum(t);
  t = max(mix(vec3(L), t, uSaturation), 0.0);
  // outlines
  if (uOutline > 0.0) {
    vec2 px = uOutline * 1.5 / vec2(textureSize(uDepth, 0));
    float d0 = ld;
    float e = 0.0;
    for (int i = 0; i < 4; i++) {
      vec2 o = vec2(i == 0 ? 1.0 : i == 1 ? -1.0 : 0.0, i == 2 ? 1.0 : i == 3 ? -1.0 : 0.0) * px;
      float dn = linDepth(texture(uDepth, uv + o).r);
      e = max(e, abs(dn - d0) / max(d0, 0.01));
    }
    float line = smoothstep(0.02, 0.08, e);
    t = mix(t, vec3(0.05), line * 0.85);
  }
  // vignette
  float dv = length(uv - 0.5) * 1.414;
  t *= 1.0 - uVignette * 0.75 * smoothstep(0.35, 1.0, dv);
  t = pow(clamp(t, 0.0, 1.0), vec3(1.0 / 2.2));
  oColor = vec4(t, uKeepAlpha == 1 ? src.a : 1.0);
}
"#;
