import { A as e, C as t, D as n, E as r, F as i, I as a, L as o, M as s, N as c, O as l, P as u, S as d, T as f, a as p, b as m, c as h, g, h as _, i as v, j as y, k as b, o as ee, r as te, t as ne, w as re, x as ie, y as ae } from "../share.js";
//#region src/writer/zxing_writer.js
function oe(e, t, n) {
	return t in e ? Object.defineProperty(e, t, {
		value: n,
		enumerable: !0,
		configurable: !0,
		writable: !0
	}) : e[t] = n, e;
}
async function x() {
	let e = arguments.length > 0 && arguments[0] !== void 0 ? arguments[0] : {};
	var t, n, r, i = e, a = !!globalThis.window, o = !!globalThis.WorkerGlobalScope;
	(n = globalThis.process) != null && (t = n.versions) != null && t.node && ((r = globalThis.process) == null || r.type);
	var s = "./this.program", c = (e, t) => {
		throw t;
	}, l = void 0, u = "";
	function d(e) {
		return i.locateFile ? i.locateFile(e, u) : u + e;
	}
	var f, p;
	if (a || o || typeof fetch == "function") {
		try {
			u = new URL(".", l).href;
		} catch {}
		o && (p = (e) => {
			var t = new XMLHttpRequest();
			return t.open("GET", e, !1), t.responseType = "arraybuffer", t.send(null), new Uint8Array(t.response);
		}), f = async (e) => {
			var t = await fetch(e, { credentials: "same-origin" });
			if (t.ok) return t.arrayBuffer();
			throw Error(t.status + " : " + t.url);
		};
	}
	var m = console.log.bind(console), h = console.error.bind(console), g, _ = !1, v;
	class y {}
	class b extends y {
		constructor(e) {
			super(), this.excPtr = e;
		}
	}
	function ee() {
		return zn.buffer;
	}
	function te() {
		var e;
		if (!(C != null && (e = C.buffer) != null && e.resizable)) {
			var t = ee();
			C = new Int8Array(t), G = new Int16Array(t), i.HEAPU8 = M = new Uint8Array(t), K = new Uint16Array(t), q = new Int32Array(t), k = new Uint32Array(t), at = new Float32Array(t), ot = new Float64Array(t);
		}
	}
	function ne() {
		var e = i.preRun;
		e && (typeof e == "function" && (e = [e]), ge.push(...e)), me(ge);
	}
	function re() {
		pr.ra();
	}
	function ie() {
		var e = i.postRun;
		e && (typeof e == "function" && (e = [e]), he.push(...e)), me(he);
	}
	function ae(e) {
		var t;
		throw (t = i.onAbort) == null || t.call(i, e), e = `Aborted(${e})`, h(e), _ = !0, e += ". Build with -sASSERTIONS for more info.", new WebAssembly.RuntimeError(e);
	}
	var x;
	function S() {
		return d("zxing_writer.wasm");
	}
	function se(e) {
		if (e == x && g) return new Uint8Array(g);
		if (p) return p(e);
		throw "both async and sync fetching of the wasm failed";
	}
	async function ce(e) {
		if (!g) try {
			var t = await f(e);
			return new Uint8Array(t);
		} catch {}
		return se(e);
	}
	async function le(e, t) {
		try {
			var n = await ce(e);
			return await WebAssembly.instantiate(n, t);
		} catch (e) {
			h(`failed to asynchronously prepare wasm: ${e}`), ae(e);
		}
	}
	async function ue(e, t, n) {
		if (!e) try {
			var r = fetch(t, { credentials: "same-origin" });
			return await WebAssembly.instantiateStreaming(r, n);
		} catch (e) {
			h(`wasm streaming compile failed: ${e}`), h("falling back to ArrayBuffer instantiation");
		}
		return le(t, n);
	}
	function de() {
		return { a: Hn };
	}
	async function fe() {
		function e(e) {
			return pr = e.exports, Vn(pr), te(), pr;
		}
		function t(t) {
			return e(t.instance);
		}
		var n = de(), r = i.instantiateWasm;
		return r ? new Promise((t) => {
			r(n, (n) => t(e(n)));
		}) : (x != null || (x = S()), t(await ue(g, x, n)));
	}
	class pe {
		constructor(e) {
			oe(this, "name", "ExitStatus"), this.message = `Program terminated with exit(${e})`, this.status = e;
		}
	}
	var C, me = (e) => {
		for (; e.length > 0;) e.shift()(i);
	}, he = [], ge = [], _e = !0, w = (e) => Mn(e), T = () => Pn(), ve = [], E = (e) => {
		var t = ve[e];
		return t || (ve[e] = t = Bn.get(e)), t;
	}, ye = (e, t) => E(e)(t), D = [], O = 0, k;
	class be {
		set_type(e) {
			k[this.ptr + 4 >> 2] = e;
		}
		get_type() {
			return k[this.ptr + 4 >> 2];
		}
		set_destructor(e) {
			k[this.ptr + 8 >> 2] = e;
		}
		get_destructor() {
			return k[this.ptr + 8 >> 2];
		}
		set_caught(e) {
			e = +!!e, C[this.ptr + 12] = e;
		}
		get_caught() {
			return C[this.ptr + 12] != 0;
		}
		set_rethrown(e) {
			e = +!!e, C[this.ptr + 13] = e;
		}
		get_rethrown() {
			return C[this.ptr + 13] != 0;
		}
		init(e, t) {
			this.set_adjusted_ptr(0), this.set_type(e), this.set_destructor(t);
		}
		set_adjusted_ptr(e) {
			k[this.ptr + 16 >> 2] = e;
		}
		get_adjusted_ptr() {
			return k[this.ptr + 16 >> 2];
		}
		constructor(e) {
			this.excPtr = e, this.ptr = e - 24;
		}
	}
	var xe = (e) => {
		var t = new be(e);
		return t.get_caught() || (t.set_caught(!0), O--), t.set_rethrown(!1), D.push(t), Rn(e);
	}, A = null, Se = () => {
		$(0, 0);
		var e = D.pop();
		Fn(e.excPtr), A = null;
	}, j = (e) => jn(e), Ce = (e) => {
		var t = A == null ? void 0 : A.excPtr;
		if (!t) return j(0), 0;
		var n = new be(t);
		n.set_adjusted_ptr(t);
		var r = n.get_type();
		if (!r) return j(0), t;
		for (var i of e) {
			if (!i || i === r) break;
			var a = n.ptr + 16;
			if (Ln(i, r, a)) return j(i), t;
		}
		return j(r), t;
	}, we = () => Ce([]), Te = (e) => Ce([e]), Ee = (e, t) => Ce([e, t]), De = (e) => {
		throw e;
	}, Oe = () => {
		D.length || ae("no exception to throw");
		var e = D[D.length - 1], t = e.excPtr;
		e.set_rethrown(!0), e.set_caught(!1), O++, In(t), t = A = new b(t), De(t);
	}, ke = (e, t, n) => {
		new be(e).init(t, n), In(e), e = A = new b(e), O++, De(e);
	}, Ae = () => O, je = (e) => {
		throw e;
	}, Me = (e) => {
		e = A == null ? A = new b(e) : A, je(e);
	}, Ne = globalThis.TextDecoder && new TextDecoder(), Pe = (e, t, n, r) => {
		var i = t + n;
		if (r) return i;
		for (; e[t] && !(t >= i);) ++t;
		return t;
	}, Fe = function(e) {
		let t = arguments.length > 1 && arguments[1] !== void 0 ? arguments[1] : 0, n = arguments.length > 2 ? arguments[2] : void 0, r = arguments.length > 3 ? arguments[3] : void 0;
		var i = Pe(e, t, n, r);
		if (i - t > 16 && e.buffer && Ne) return Ne.decode(e.subarray(t, i));
		for (var a = ""; t < i;) {
			var o = e[t++];
			if (!(o & 128)) {
				a += String.fromCharCode(o);
				continue;
			}
			var s = e[t++] & 63;
			if ((o & 224) == 192) {
				a += String.fromCharCode((o & 31) << 6 | s);
				continue;
			}
			var c = e[t++] & 63;
			if (o = (o & 240) == 224 ? (o & 15) << 12 | s << 6 | c : (o & 7) << 18 | s << 12 | c << 6 | e[t++] & 63, o < 65536) a += String.fromCharCode(o);
			else {
				var l = o - 65536;
				a += String.fromCharCode(55296 | l >> 10, 56320 | l & 1023);
			}
		}
		return a;
	}, M, Ie = (e, t, n) => e ? Fe(M, e, t, n) : "";
	function Le(e, t, n) {
		return 0;
	}
	function Re(e, t, n) {
		return 0;
	}
	var ze = (e, t, n) => {};
	function Be(e, t, n, r) {}
	var Ve = (e, t) => {}, He = () => ae(""), N = {}, Ue = (e) => {
		for (; e.length;) {
			var t = e.pop();
			e.pop()(t);
		}
	};
	function P(e) {
		return this.fromWireType(k[e >> 2]);
	}
	var F = {}, I = {}, L = {};
	class We extends Error {
		constructor(e) {
			super(e), this.name = "InternalError";
		}
	}
	var Ge = (e) => {
		throw new We(e);
	}, Ke = (e, t, n) => {
		e.forEach((e) => L[e] = t);
		function r(t) {
			var r = n(t);
			r.length !== e.length && Ge("Mismatched type converter count");
			for (var i = 0; i < e.length; ++i) B(e[i], r[i]);
		}
		var i = Array(t.length), a = [], o = 0;
		for (let [e, n] of t.entries()) I.hasOwnProperty(n) ? i[e] = I[n] : (a.push(n), F.hasOwnProperty(n) || (F[n] = []), F[n].push(() => {
			i[e] = I[n], ++o, o === a.length && r(i);
		}));
		a.length === 0 && r(i);
	}, qe = (e) => Nn(e), Je = (e, t) => M.fill(0, e, e + t), Ye = (e) => {
		var t = N[e];
		delete N[e];
		var n = t.rawConstructor, r = t.rawDestructor, i = t.valueSize, a = t.isTrivial, o = t.fields, s = o.map((e) => e.getterReturnType).concat(o.map((e) => e.setterArgumentType));
		Ke([e], s, (e) => {
			var s = {};
			for (var [c, l] of o.entries()) {
				let t = e[c], n = l.getter, r = l.getterContext, i = e[c + o.length], a = l.setter, d = l.setterContext;
				var u = i.destructorFunction === null && !i.argStackAlloc ? (e, t) => a(d, e, i.toWireType(null, t)) : (e, t) => {
					var n = [];
					a(d, e, i.toWireType(n, t)), Ue(n);
				};
				s[l.fieldName] = {
					read: (e) => t.fromWireType(n(r, e)),
					write: u,
					optional: t.optional
				};
			}
			return [{
				name: t.name,
				fromWireType: (e) => {
					var t = {};
					for (var n in s) t[n] = s[n].read(e);
					return r(e), t;
				},
				toWireType: (e, t) => {
					for (var o in s) if (!(o in t) && !s[o].optional) throw TypeError(`Missing field: "${o}"`);
					var c;
					for (o in a && !e ? (c = qe(i), Je(c, i)) : (c = n(), e && e.push(r, c)), s) s[o].write(c, t[o]);
					return c;
				},
				readValueFromPointer: P,
				destructorFunction: a ? null : r,
				argStackAlloc: a
			}];
		});
	}, Xe = (e, t, n, r, i) => {}, R = (e) => {
		for (var t = "";;) {
			var n = M[e++];
			if (!n) return t;
			t += String.fromCharCode(n);
		}
	};
	class Ze extends Error {
		constructor(e) {
			super(e), this.name = "BindingError";
		}
	}
	var z = (e) => {
		throw new Ze(e);
	};
	function Qe(e, t) {
		let n = arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : {};
		var r = t.name;
		if (e || z(`type "${r}" must have a positive integer typeid pointer`), I.hasOwnProperty(e)) {
			if (n.ignoreDuplicateRegistrations) return;
			z(`Cannot register type '${r}' twice`);
		}
		if (I[e] = t, delete L[e], F.hasOwnProperty(e)) {
			var i = F[e];
			delete F[e], i.forEach((e) => e());
		}
	}
	function B(e, t) {
		return Qe(e, t, arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : {});
	}
	var $e = (e, t, n, r) => {
		t = R(t), B(e, {
			name: t,
			fromWireType: function(e) {
				return !!e;
			},
			toWireType: function(e, t) {
				return t ? n : r;
			},
			readValueFromPointer: function(e) {
				return this.fromWireType(M[e]);
			},
			destructorFunction: null
		});
	}, et = [], V = [
		0,
		1,
		,
		1,
		null,
		1,
		!0,
		1,
		!1,
		1
	], tt = [], nt = (e) => {
		if (e > 9 && --V[e + 1] === 0) {
			var t = V[e];
			V[e] = void 0;
			var n = tt[e];
			n && (tt[e] = void 0, n(t)), et.push(e);
		}
	}, H = {
		toValue: (e) => (e || z(`Cannot use deleted val. handle = ${e}`), V[e]),
		toHandle: (e) => {
			switch (e) {
				case void 0: return 2;
				case null: return 4;
				case !0: return 6;
				case !1: return 8;
				default: {
					let t = et.pop() || V.length;
					return V[t] = e, V[t + 1] = 1, t;
				}
			}
		}
	}, rt = {
		name: "emscripten::val",
		fromWireType: (e) => {
			var t = H.toValue(e);
			return nt(e), t;
		},
		toWireType: (e, t) => H.toHandle(t),
		readValueFromPointer: P,
		destructorFunction: null
	}, it = (e) => B(e, rt), at, ot, st = (e, t) => {
		switch (t) {
			case 4: return function(e) {
				return this.fromWireType(at[e >> 2]);
			};
			case 8: return function(e) {
				return this.fromWireType(ot[e >> 3]);
			};
			default: throw TypeError(`invalid float width (${t}): ${e}`);
		}
	}, ct = (e, t, n) => {
		t = R(t), B(e, {
			name: t,
			fromWireType: (e) => e,
			toWireType: (e, t) => t,
			readValueFromPointer: st(t, n),
			destructorFunction: null
		});
	}, lt = (e, t) => Object.defineProperty(t, "name", { value: e });
	function ut(e) {
		for (var t = 1; t < e.length; ++t) if (e[t] !== null && e[t].destructorFunction === void 0) return !0;
		return !1;
	}
	function dt(e) {
		for (var t = 1; t < e.length; ++t) if (e[t] !== null && e[t].argStackAlloc) return !0;
		return !1;
	}
	function ft(e, t, n, r, i, a) {
		var o = t.length;
		o < 2 && z("argTypes array size mismatch! Must at least get return value and receiver (this) types!");
		var s = t[1] !== null && n !== null, c = ut(t), l = dt(t), u = l && !a && !c;
		l && !u && (c = !0);
		var d = !t[0].isVoid, f = o - 2, p = Array(f), m = [], h = [];
		return lt(e, function() {
			var e = [...arguments];
			h.length = 0;
			var n;
			u && (n = T());
			var a, o;
			try {
				m.length = s ? 2 : 1, m[0] = i, s && (a = t[1].toWireType(h, this), m[1] = a);
				for (var l = 0; l < f; ++l) {
					var g = t[l + 2];
					p[l] = g.toWireType(u && g.argStackAlloc ? null : h, e[l]), m.push(p[l]);
				}
				o = r(...m);
			} finally {
				u && w(n);
			}
			function _(e) {
				if (c) Ue(h);
				else for (var n = s ? 1 : 2; n < t.length; n++) {
					var r = n === 1 ? a : p[n - 2];
					t[n].destructorFunction !== null && t[n].destructorFunction(r);
				}
				if (d) return t[0].fromWireType(e);
			}
			return _(o);
		});
	}
	var pt = (e, t, n) => {
		if (e[t].overloadTable === void 0) {
			var r = e[t];
			e[t] = function() {
				var r = [...arguments];
				return e[t].overloadTable.hasOwnProperty(r.length) || z(`Function '${n}' called with an invalid number of arguments (${r.length}) - expects one of (${e[t].overloadTable})!`), e[t].overloadTable[r.length].apply(this, r);
			}, e[t].overloadTable = [], e[t].overloadTable[r.argCount] = r;
		}
	}, mt = (e, t, n) => {
		i.hasOwnProperty(e) ? ((n === void 0 || i[e].overloadTable !== void 0 && i[e].overloadTable[n] !== void 0) && z(`Cannot register public name '${e}' twice`), pt(i, e, e), i[e].overloadTable.hasOwnProperty(n) && z(`Cannot register multiple overloads of a function with the same number of arguments (${n})!`), i[e].overloadTable[n] = t) : (i[e] = t, i[e].argCount = n);
	}, ht = (e, t) => {
		for (var n = [], r = 0; r < e; r++) n.push(k[t + r * 4 >> 2]);
		return n;
	}, gt = (e, t, n) => {
		i.hasOwnProperty(e) || Ge("Replacing nonexistent public symbol"), i[e].overloadTable !== void 0 && n !== void 0 ? i[e].overloadTable[n] = t : (i[e] = t, i[e].argCount = n);
	}, U = {}, _t = (e, t, n) => {
		e = e.replace(/p/g, "i");
		var r = U[e];
		return r(t, ...n);
	}, vt = function(e, t) {
		let n = arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : [];
		if (arguments.length > 3 && arguments[3] !== void 0 && arguments[3], e.includes("j")) return _t(e, t, n);
		var r = E(t)(...n);
		function i(e) {
			return e;
		}
		return i(r);
	}, yt = function(e, t) {
		let n = arguments.length > 2 && arguments[2] !== void 0 && arguments[2];
		return function() {
			return vt(e, t, [...arguments], n);
		};
	}, W = function(e, t) {
		arguments.length > 2 && arguments[2] !== void 0 && arguments[2], e = R(e);
		function n() {
			return e.includes("j") ? yt(e, t) : E(t);
		}
		var r = n();
		return typeof r != "function" && z(`unknown function pointer with signature ${e}: ${t}`), r;
	};
	class bt extends Error {}
	var xt = (e) => {
		var t = On(e), n = R(t);
		return Q(t), n;
	}, St = (e, t) => {
		var n = [], r = {};
		function i(e) {
			if (!r[e] && !I[e]) {
				if (L[e]) {
					L[e].forEach(i);
					return;
				}
				n.push(e), r[e] = !0;
			}
		}
		throw t.forEach(i), new bt(`${e}: ` + n.map(xt).join([", "]));
	}, Ct = (e) => {
		e = e.trim();
		let t = e.indexOf("(");
		return t === -1 ? e : e.slice(0, t);
	}, wt = (e, t, n, r, i, a, o, s) => {
		var c = ht(t, n);
		e = R(e), e = Ct(e), i = W(r, i, o), mt(e, function() {
			St(`Cannot call ${e} due to unbound types`, c);
		}, t - 1), Ke([], c, (n) => {
			var r = [n[0], null].concat(n.slice(1));
			return gt(e, ft(e, r, null, i, a, o), t - 1), [];
		});
	}, G, K, q, Tt = (e, t, n) => {
		switch (t) {
			case 1: return n ? (e) => C[e] : (e) => M[e];
			case 2: return n ? (e) => G[e >> 1] : (e) => K[e >> 1];
			case 4: return n ? (e) => q[e >> 2] : (e) => k[e >> 2];
			default: throw TypeError(`invalid integer width (${t}): ${e}`);
		}
	}, Et = (e, t, n, r, i) => {
		t = R(t);
		let a = r === 0, o = (e) => e;
		if (a) {
			var s = 32 - 8 * n;
			o = (e) => e << s >>> s, i = o(i);
		}
		B(e, {
			name: t,
			fromWireType: o,
			toWireType: (e, t) => t,
			readValueFromPointer: Tt(t, n, r !== 0),
			destructorFunction: null
		});
	}, Dt = (e, t, n) => {
		var r = [
			Int8Array,
			Uint8Array,
			Int16Array,
			Uint16Array,
			Int32Array,
			Uint32Array,
			Float32Array,
			Float64Array
		][t];
		function i(e) {
			var t = k[e >> 2], n = k[e + 4 >> 2];
			return new r(C.buffer, n, t);
		}
		n = R(n), B(e, {
			name: n,
			fromWireType: i,
			readValueFromPointer: i
		}, { ignoreDuplicateRegistrations: !0 });
	}, Ot = (e, t, n, r) => {
		if (!(r > 0)) return 0;
		for (var i = n, a = n + r - 1, o = 0; o < e.length; ++o) {
			var s = e.codePointAt(o);
			if (s <= 127) {
				if (n >= a) break;
				t[n++] = s;
			} else if (s <= 2047) {
				if (n + 1 >= a) break;
				t[n++] = 192 | s >> 6, t[n++] = 128 | s & 63;
			} else if (s <= 65535) {
				if (n + 2 >= a) break;
				t[n++] = 224 | s >> 12, t[n++] = 128 | s >> 6 & 63, t[n++] = 128 | s & 63;
			} else {
				if (n + 3 >= a) break;
				t[n++] = 240 | s >> 18, t[n++] = 128 | s >> 12 & 63, t[n++] = 128 | s >> 6 & 63, t[n++] = 128 | s & 63, o++;
			}
		}
		return t[n] = 0, n - i;
	}, J = (e, t, n) => Ot(e, M, t, n), kt = (e) => {
		for (var t = 0, n = 0; n < e.length; ++n) {
			var r = e.charCodeAt(n);
			r <= 127 ? t++ : r <= 2047 ? t += 2 : r >= 55296 && r <= 57343 ? (t += 4, ++n) : t += 3;
		}
		return t;
	}, At = (e, t) => {
		t = R(t);
		var n = !0;
		B(e, {
			name: t,
			fromWireType(e) {
				var t = k[e >> 2], r = e + 4, i;
				if (n) i = Ie(r, t, !0);
				else {
					i = "";
					for (var a = 0; a < t; ++a) i += String.fromCharCode(M[r + a]);
				}
				return Q(e), i;
			},
			toWireType(e, t) {
				t instanceof ArrayBuffer && (t = new Uint8Array(t));
				var r, i = typeof t == "string";
				i || ArrayBuffer.isView(t) && t.BYTES_PER_ELEMENT == 1 || z("Cannot pass non-string to std::string"), r = n && i ? kt(t) : t.length;
				var a = kn(4 + r + 1), o = a + 4;
				if (k[a >> 2] = r, i) {
					if (n) J(t, o, r + 1);
					else for (var s = 0; s < r; ++s) {
						var c = t.charCodeAt(s);
						c > 255 && (Q(a), z("String has UTF-16 code units that do not fit in 8 bits")), M[o + s] = c;
					}
				} else M.set(t, o);
				return e !== null && e.push(Q, a), a;
			},
			readValueFromPointer: P,
			destructorFunction(e) {
				Q(e);
			}
		});
	}, jt = globalThis.TextDecoder ? new TextDecoder("utf-16le") : void 0, Mt = (e, t, n) => {
		var r = e >> 1, i = Pe(K, r, t / 2, n);
		if (i - r > 16 && jt) return jt.decode(K.subarray(r, i));
		for (var a = "", o = r; o < i; ++o) {
			var s = K[o];
			a += String.fromCharCode(s);
		}
		return a;
	}, Nt = function(e, t) {
		let n = arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : 2147483647;
		if (n < 2) return 0;
		n -= 2;
		for (var r = t, i = n < e.length * 2 ? n / 2 : e.length, a = 0; a < i; ++a) {
			var o = e.charCodeAt(a);
			G[t >> 1] = o, t += 2;
		}
		return G[t >> 1] = 0, t - r;
	}, Pt = (e) => e.length * 2, Ft = (e, t, n) => {
		for (var r = "", i = e >> 2, a = 0; !(a >= t / 4); a++) {
			var o = k[i + a];
			if (!o && !n) break;
			r += String.fromCodePoint(o);
		}
		return r;
	}, It = function(e, t) {
		let n = arguments.length > 2 && arguments[2] !== void 0 ? arguments[2] : 2147483647;
		if (n < 4) return 0;
		for (var r = t, i = r + n - 4, a = 0; a < e.length; ++a) {
			var o = e.codePointAt(a);
			if (o > 65535 && a++, q[t >> 2] = o, t += 4, t + 4 > i) break;
		}
		return q[t >> 2] = 0, t - r;
	}, Lt = (e) => {
		for (var t = 0, n = 0; n < e.length; ++n) e.codePointAt(n) > 65535 && n++, t += 4;
		return t;
	}, Rt = (e, t, n) => {
		n = R(n);
		var r, i, a;
		t === 2 ? (r = Mt, i = Nt, a = Pt) : (r = Ft, i = It, a = Lt), B(e, {
			name: n,
			fromWireType: (e) => {
				var n = k[e >> 2], i = r(e + 4, n * t, !0);
				return Q(e), i;
			},
			toWireType: (e, r) => {
				typeof r != "string" && z(`Cannot pass non-string to C++ string type ${n}`);
				var o = a(r), s = kn(4 + o + t);
				return k[s >> 2] = o / t, i(r, s + 4, o + t), e !== null && e.push(Q, s), s;
			},
			readValueFromPointer: P,
			destructorFunction(e) {
				Q(e);
			}
		});
	}, zt = (e, t, n, r, i, a, o, s) => {
		N[e] = {
			name: R(t),
			rawConstructor: W(n, r),
			rawDestructor: W(i, a),
			valueSize: o,
			isTrivial: !!s,
			fields: []
		};
	}, Bt = (e, t, n, r, i, a, o, s, c, l) => {
		N[e].fields.push({
			fieldName: R(t),
			getterReturnType: n,
			getter: W(r, i),
			getterContext: a,
			setterArgumentType: o,
			setter: W(s, c),
			setterContext: l
		});
	}, Vt = (e, t) => {
		t = R(t), B(e, {
			isVoid: !0,
			name: t,
			fromWireType: () => void 0,
			toWireType: (e, t) => void 0
		});
	}, Ht = 0, Ut = () => {
		_e = !1, Ht = 0;
	}, Wt = [], Gt = (e) => {
		var t = Wt.length;
		return Wt.push(e), t;
	}, Kt = (e, t) => {
		var n = I[e];
		return n === void 0 && z(`${t} has unknown type ${xt(e)}`), n;
	}, qt = (e, t) => {
		for (var n = Array(e), r = 0; r < e; ++r) n[r] = Kt(k[t + r * 4 >> 2], `parameter ${r}`);
		return n;
	}, Jt = (e, t, n) => {
		var r = [], i = e(r, n);
		return r.length && (k[t >> 2] = H.toHandle(r)), i;
	}, Yt = {}, Xt = (e) => {
		var t = Yt[e];
		return t === void 0 ? R(e) : t;
	}, Zt = (e, t, n) => {
		var [r, ...i] = qt(e, t), a = r.toWireType.bind(r), o = i.map((e) => e.readValueFromPointer.bind(e));
		e--;
		var s = Array(e);
		return Gt(lt(`methodCaller<(${i.map((e) => e.name)}) => ${r.name}>`, (t, r, i, c) => {
			for (var l = 0, u = 0; u < e; ++u) s[u] = o[u](c + l), l += 8;
			var d;
			switch (n) {
				case 0:
					d = H.toValue(t).apply(null, s);
					break;
				case 2:
					d = Reflect.construct(H.toValue(t), s);
					break;
				case 3:
					d = s[0];
					break;
				case 1: d = H.toValue(t)[Xt(r)](...s);
			}
			return Jt(a, i, d);
		}));
	}, Qt = (e) => e ? (e = Xt(e), H.toHandle(globalThis[e])) : H.toHandle(globalThis), $t = (e) => {
		e > 9 && (V[e + 1] += 1);
	}, en = (e, t, n, r, i) => Wt[e](t, n, r, i), tn = (e) => {
		Ue(H.toValue(e)), nt(e);
	}, Y = {}, nn = (e) => {
		if (e instanceof pe || e == "unwind") return v;
		c(1, e);
	}, rn = () => _e || Ht > 0, an = (e) => {
		if (v = e, !rn()) {
			var t;
			(t = i.onExit) == null || t.call(i, e), _ = !0;
		}
		c(e, new pe(e));
	}, on = (e, t) => {
		v = e, an(e);
	}, sn = () => {
		if (!rn()) try {
			on(v);
		} catch (e) {
			nn(e);
		}
	}, cn = function(e) {
		var t = [...arguments].slice(1);
		if (!_) try {
			return e(...t);
		} catch (e) {
			nn(e);
		} finally {
			sn();
		}
	}, ln = () => performance.now(), un = (e, t) => (Y[e] && (clearTimeout(Y[e].id), delete Y[e]), t && (Y[e] = {
		id: setTimeout(() => {
			delete Y[e], cn(An, e, ln());
		}, t),
		timeout_ms: t
	}), 0), dn = (e, t, n, r) => {
		var i = (/* @__PURE__ */ new Date()).getFullYear(), a = new Date(i, 0, 1), o = new Date(i, 6, 1), s = a.getTimezoneOffset(), c = o.getTimezoneOffset(), l = Math.max(s, c);
		k[e >> 2] = l * 60, q[t >> 2] = Number(s != c);
		var u = (e) => {
			var t = e >= 0 ? "-" : "+", n = Math.abs(e);
			return `UTC${t}${String(Math.floor(n / 60)).padStart(2, "0")}${String(n % 60).padStart(2, "0")}`;
		}, d = u(s), f = u(c);
		c < s ? (J(d, n, 17), J(f, r, 17)) : (J(d, r, 17), J(f, n, 17));
	}, fn = () => 2147483648, pn = (e, t) => Math.ceil(e / t) * t, mn = (e) => {
		var t = (e - zn.buffer.byteLength + 65535) / 65536 | 0;
		try {
			return zn.grow(t), te(), 1;
		} catch {}
	}, hn = (e) => {
		var t = M.length;
		e >>>= 0;
		var n = fn();
		if (e > n) return !1;
		for (var r = 1; r <= 4; r *= 2) {
			var i = t * (1 + .2 / r);
			if (i = Math.min(i, e + 100663296), mn(Math.min(n, pn(Math.max(e, i), 65536)))) return !0;
		}
		return !1;
	}, gn = {}, _n = () => s, X = () => {
		if (!X.strings) {
			var e, t, n = {
				USER: "web_user",
				LOGNAME: "web_user",
				PATH: "/",
				PWD: "/",
				HOME: "/home/web_user",
				LANG: ((e = (t = globalThis.navigator) == null ? void 0 : t.language) == null ? "C" : e).replace("-", "_") + ".UTF-8",
				_: _n()
			};
			for (var r in gn) gn[r] === void 0 ? delete n[r] : n[r] = gn[r];
			var i = [];
			for (var r in n) i.push(`${r}=${n[r]}`);
			X.strings = i;
		}
		return X.strings;
	}, vn = (e, t) => {
		var n = 0, r = 0;
		for (var i of X()) {
			var a = t + n;
			k[e + r >> 2] = a, n += J(i, a, Infinity) + 1, r += 4;
		}
		return 0;
	}, yn = (e, t) => {
		var n = X();
		k[e >> 2] = n.length;
		var r = 0;
		for (var i of n) r += kt(i) + 1;
		return k[t >> 2] = r, 0;
	}, bn = (e) => 52, xn = (e, t, n, r) => 52, Sn = (e, t) => t + 2097152 >>> 0 < 4194305 - !!e ? (e >>> 0) + t * 4294967296 : NaN;
	function Cn(e, t, n, r, i) {
		return Sn(t, n), 70;
	}
	var wn = [
		null,
		[],
		[]
	], Tn = (e, t) => {
		var n = wn[e];
		!t || t === 10 ? ((e === 1 ? m : h)(Fe(n)), n.length = 0) : n.push(t);
	}, En = (e, t, n, r) => {
		for (var i = 0, a = 0; a < n; a++) {
			var o = k[t >> 2], s = k[t + 4 >> 2];
			t += 8;
			for (var c = 0; c < s; c++) Tn(e, M[o + c]);
			i += s;
		}
		return k[r >> 2] = i, 0;
	}, Dn = (e) => e;
	i.noExitRuntime && (_e = i.noExitRuntime), i.print && (m = i.print), i.printErr && (h = i.printErr), i.wasmBinary && (g = i.wasmBinary), i.arguments && i.arguments, i.thisProgram && (s = i.thisProgram);
	var Z = i.preInit;
	if (Z) for (typeof Z == "function" && (i.preInit = Z = [Z]); Z.length > 0;) Z.shift()();
	var On, Q, kn, An, $, jn, Mn, Nn, Pn, Fn, In, Ln, Rn, zn, Bn;
	function Vn(e) {
		On = e.sa, Q = i._free = e.ta, kn = i._malloc = e.ua, An = e.wa, $ = e.xa, jn = e.ya, Mn = e.za, Nn = e.Aa, Pn = e.Ba, Fn = e.Ca, In = e.Da, Ln = e.Ea, Rn = e.Fa, U.jiji = e.Ga, U.viijii = e.Ha, U.iiiiij = e.Ia, U.iiiiijj = e.Ja, U.iiiiiijj = e.Ka, zn = e.qa, Bn = e.va;
	}
	var Hn = {
		aa: ye,
		t: xe,
		u: Se,
		b: we,
		h: Te,
		v: Ee,
		Z: Oe,
		p: ke,
		Y: Ae,
		d: Me,
		M: Le,
		ha: Re,
		fa: ze,
		ia: Be,
		$: Ve,
		ea: He,
		P: Ye,
		R: Xe,
		la: $e,
		ja: it,
		O: ct,
		F: wt,
		s: Et,
		m: Dt,
		ka: At,
		E: Rt,
		D: zt,
		T: Bt,
		ma: Vt,
		da: Ut,
		pa: Zt,
		ca: nt,
		N: Qt,
		G: $t,
		oa: en,
		na: tn,
		U: un,
		V: dn,
		_: hn,
		W: vn,
		X: yn,
		K: bn,
		ga: xn,
		Q: Cn,
		L: En,
		H: or,
		A: Qn,
		I: ar,
		n: lr,
		a: Yn,
		c: qn,
		g: Zn,
		k: sr,
		B: rr,
		q: Xn,
		x: $n,
		z: ur,
		j: Jn,
		i: Un,
		f: Wn,
		e: Kn,
		r: nr,
		w: Gn,
		S: ir,
		o: cr,
		C: tr,
		l: er,
		y: dr,
		J: Dn,
		ba: an
	};
	function Un(e, t) {
		var n = T();
		try {
			E(e)(t);
		} catch (e) {
			if (w(n), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function Wn(e, t, n) {
		var r = T();
		try {
			E(e)(t, n);
		} catch (e) {
			if (w(r), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function Gn(e, t, n, r, i, a) {
		var o = T();
		try {
			E(e)(t, n, r, i, a);
		} catch (e) {
			if (w(o), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function Kn(e, t, n, r) {
		var i = T();
		try {
			E(e)(t, n, r);
		} catch (e) {
			if (w(i), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function qn(e, t, n) {
		var r = T();
		try {
			return E(e)(t, n);
		} catch (e) {
			if (w(r), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function Jn(e) {
		var t = T();
		try {
			E(e)();
		} catch (e) {
			if (w(t), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function Yn(e, t) {
		var n = T();
		try {
			return E(e)(t);
		} catch (e) {
			if (w(n), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function Xn(e, t, n, r, i, a, o) {
		var s = T();
		try {
			return E(e)(t, n, r, i, a, o);
		} catch (e) {
			if (w(s), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function Zn(e, t, n, r) {
		var i = T();
		try {
			return E(e)(t, n, r);
		} catch (e) {
			if (w(i), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function Qn(e, t, n, r, i, a) {
		var o = T();
		try {
			return E(e)(t, n, r, i, a);
		} catch (e) {
			if (w(o), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function $n(e, t, n, r, i, a, o, s) {
		var c = T();
		try {
			return E(e)(t, n, r, i, a, o, s);
		} catch (e) {
			if (w(c), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function er(e, t, n, r, i, a, o, s, c, l, u) {
		var d = T();
		try {
			E(e)(t, n, r, i, a, o, s, c, l, u);
		} catch (e) {
			if (w(d), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function tr(e, t, n, r, i, a, o, s, c) {
		var l = T();
		try {
			E(e)(t, n, r, i, a, o, s, c);
		} catch (e) {
			if (w(l), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function nr(e, t, n, r, i) {
		var a = T();
		try {
			E(e)(t, n, r, i);
		} catch (e) {
			if (w(a), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function rr(e, t, n, r, i, a) {
		var o = T();
		try {
			return E(e)(t, n, r, i, a);
		} catch (e) {
			if (w(o), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function ir(e, t, n, r, i, a, o) {
		var s = T();
		try {
			E(e)(t, n, r, i, a, o);
		} catch (e) {
			if (w(s), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function ar(e, t, n, r) {
		var i = T();
		try {
			return E(e)(t, n, r);
		} catch (e) {
			if (w(i), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function or(e, t, n, r) {
		var i = T();
		try {
			return E(e)(t, n, r);
		} catch (e) {
			if (w(i), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function sr(e, t, n, r, i) {
		var a = T();
		try {
			return E(e)(t, n, r, i);
		} catch (e) {
			if (w(a), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function cr(e, t, n, r, i, a, o, s) {
		var c = T();
		try {
			E(e)(t, n, r, i, a, o, s);
		} catch (e) {
			if (w(c), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function lr(e) {
		var t = T();
		try {
			return E(e)();
		} catch (e) {
			if (w(t), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function ur(e, t, n, r, i, a, o, s, c, l, u, d) {
		var f = T();
		try {
			return E(e)(t, n, r, i, a, o, s, c, l, u, d);
		} catch (e) {
			if (w(f), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	function dr(e, t, n, r, i, a, o, s, c, l, u, d, f, p, m, h) {
		var g = T();
		try {
			E(e)(t, n, r, i, a, o, s, c, l, u, d, f, p, m, h);
		} catch (e) {
			if (w(g), !(e instanceof y)) throw e;
			$(1, 0);
		}
	}
	async function fr() {
		var e;
		ne();
		var t = i.setStatus;
		t && (t("Running..."), await new Promise((e) => setTimeout(e, 1)), setTimeout(t, 1, "")), !_ && (re(), (e = i.onRuntimeInitialized) == null || e.call(i), ie());
	}
	var pr = await fe();
	return await fr(), i;
}
//#endregion
//#region src/writer/index.ts
function S(e) {
	return p(x, e);
}
function se() {
	return ee(x);
}
function ce(e) {
	return S({
		overrides: e,
		equalityFn: Object.is,
		fireImmediately: !0
	});
}
function le(e) {
	S({
		overrides: e,
		equalityFn: Object.is,
		fireImmediately: !1
	});
}
async function ue(e, t) {
	return h(x, e, t);
}
var de = "bc0969c6476b63f942142db858f985dfcc9f8fdeecd5654074031801a41c1b0d";
//#endregion
export { ae as BARCODE_FORMATS, m as BARCODE_HRI_LABELS, ie as BARCODE_META_FORMATS, d as BARCODE_SYMBOLOGIES, _ as CHARACTER_SETS, t as CREATABLE_BARCODE_FORMATS, re as GS1_BARCODE_FORMATS, f as INDUSTRIAL_BARCODE_FORMATS, r as LINEAR_BARCODE_FORMATS, n as MATRIX_BARCODE_FORMATS, l as READABLE_BARCODE_FORMATS, b as RETAIL_BARCODE_FORMATS, te as ZXING_CPP_COMMIT, de as ZXING_WASM_SHA256, v as ZXING_WASM_VERSION, e as barcodeFormats, g as characterSets, ne as defaultWriterOptions, y as encodeFormat, s as encodeFormats, c as formatToLabel, u as formatToSymbology, ce as getZXingModule, i as linearBarcodeFormats, a as matrixBarcodeFormats, S as prepareZXingModule, se as purgeZXingModule, le as setZXingModuleOverrides, o as symbologyToFormats, ue as writeBarcode };
