// MIT License. © Stefan Gustavson, Munrocket
//

fn permute(x: vec4<f32>) -> vec4<f32> {
	return (((x * 34. + 1.) * x) % (289.));
}

fn taylorInvSqrt(r: vec4<f32>) -> vec4<f32> {
	return 1.7928429 - 0.85373473 * r;
}

fn fade(t: vec3<f32>) -> vec3<f32> {
	return t * t * t * (t * (t * 6. - 15.) + 10.);
}

fn perlinNoise3(P: vec3<f32>) -> f32 {
	var Pi0: vec3<f32> = floor(P);
	var Pi1: vec3<f32> = Pi0 + vec3<f32>(1.);
	Pi0 = ((Pi0) % (289.));
	Pi1 = ((Pi1) % (289.));
	let Pf0: vec3<f32> = fract(P);
	let Pf1: vec3<f32> = Pf0 - vec3<f32>(1.);
	let ix: vec4<f32> = vec4<f32>(Pi0.x, Pi1.x, Pi0.x, Pi1.x);
	let iy: vec4<f32> = vec4<f32>(Pi0.yy, Pi1.yy);
	let iz0: vec4<f32> = Pi0.zzzz;
	let iz1: vec4<f32> = Pi1.zzzz;
	let ixy: vec4<f32> = permute(permute(ix) + iy);
	let ixy0: vec4<f32> = permute(ixy + iz0);
	let ixy1: vec4<f32> = permute(ixy + iz1);
	var gx0: vec4<f32> = ixy0 / 7.;
	var gy0: vec4<f32> = fract(floor(gx0) / 7.) - 0.5;
	gx0 = fract(gx0);
	let gz0: vec4<f32> = vec4<f32>(0.5) - abs(gx0) - abs(gy0);
	let sz0: vec4<f32> = step(gz0, vec4<f32>(0.));
	gx0 = gx0 - (sz0 * (step(vec4<f32>(0.), gx0) - vec4<f32> (0.5)));
	gy0 = gy0 - (sz0 * (step(vec4<f32> (0.), gy0) - vec4<f32> (0.5)));
	var gx1: vec4<f32> = ixy1 / 7.;
	var gy1: vec4<f32> = fract(floor(gx1) / 7.) - 0.5;
	gx1 = fract(gx1);
	let gz1: vec4<f32> = vec4<f32>(0.5) - abs(gx1) - abs(gy1);
	let sz1: vec4<f32> = step(gz1, vec4<f32>(0.));
	gx1 = gx1 - (sz1 * (step(vec4<f32> (0.), gx1) - vec4<f32> (0.5)));
	gy1 = gy1 - (sz1 * (step(vec4<f32> (0.), gy1) - vec4<f32> (0.5)));
	var g000: vec3<f32> = vec3<f32>(gx0.x, gy0.x, gz0.x);
	var g100: vec3<f32> = vec3<f32>(gx0.y, gy0.y, gz0.y);
	var g010: vec3<f32> = vec3<f32>(gx0.z, gy0.z, gz0.z);
	var g110: vec3<f32> = vec3<f32>(gx0.w, gy0.w, gz0.w);
	var g001: vec3<f32> = vec3<f32>(gx1.x, gy1.x, gz1.x);
	var g101: vec3<f32> = vec3<f32>(gx1.y, gy1.y, gz1.y);
	var g011: vec3<f32> = vec3<f32>(gx1.z, gy1.z, gz1.z);
	var g111: vec3<f32> = vec3<f32>(gx1.w, gy1.w, gz1.w);
	let norm0: vec4<f32> = taylorInvSqrt(vec4<f32>(dot(g000, g000), dot(g010, g010), dot(g100, g100), dot(g110, g110)));
	g000 = g000 * (norm0.x);
	g010 = g010 * (norm0.y);
	g100 = g100 * (norm0.z);
	g110 = g110 * (norm0.w);
	let norm1: vec4<f32> = taylorInvSqrt(vec4<f32>(dot(g001, g001), dot(g011, g011), dot(g101, g101), dot(g111, g111)));
	g001 = g001 * (norm1.x);
	g011 = g011 * (norm1.y);
	g101 = g101 * (norm1.z);
	g111 = g111 * (norm1.w);
	let n000: f32 = dot(g000, Pf0);
	let n100: f32 = dot(g100, vec3<f32>(Pf1.x, Pf0.yz));
	let n010: f32 = dot(g010, vec3<f32>(Pf0.x, Pf1.y, Pf0.z));
	let n110: f32 = dot(g110, vec3<f32>(Pf1.xy, Pf0.z));
	let n001: f32 = dot(g001, vec3<f32>(Pf0.xy, Pf1.z));
	let n101: f32 = dot(g101, vec3<f32>(Pf1.x, Pf0.y, Pf1.z));
	let n011: f32 = dot(g011, vec3<f32>(Pf0.x, Pf1.yz));
	let n111: f32 = dot(g111, Pf1);
	let fade_xyz: vec3<f32> = fade(Pf0);
	let n_z: vec4<f32> = mix(vec4<f32>(n000, n100, n010, n110), vec4<f32>(n001, n101, n011, n111), fade_xyz.z);
	let n_yz: vec2<f32> = mix(n_z.xy, n_z.zw, fade_xyz.y);
	let n_xyz: f32 = mix(n_yz.x, n_yz.y, fade_xyz.x);
	return 2.2 * n_xyz;
}