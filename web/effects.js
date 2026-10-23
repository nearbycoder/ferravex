// Original instanced billboard bridge. Clip-space centers/axes and tint come
// directly from bounded Rust frame storage; no per-particle JS object or draw.
import {spriteVertexBody, spriteFragmentBody} from './effects-shader.js';

// The Rust MAX_EFFECT_VERTICES / 3 frame budget permits at most this many
// one-triangle sprites. Instance and batch layouts remain 64 and 16 bytes.
export const MAX_SPRITE_INSTANCES = 65536;
const INSTANCE_BYTES = 64, BATCH_BYTES = 16;
const MAX_INSTANCE_BYTES = MAX_SPRITE_INSTANCES * INSTANCE_BYTES;
const validCount = count => Number.isInteger(count) && count >= 0 && count <= MAX_SPRITE_INSTANCES;
const validRange = (buffer, at, bytes) => Number.isSafeInteger(at) && at >= 0 &&
  at <= 0xffffffff && at % 4 === 0 && bytes <= buffer.byteLength && at <= buffer.byteLength - bytes;

/**
 * Bounded WebGL2 sprite uploads. `capacity` is reserved GPU bytes; `lastError`
 * describes the latest rejected operation (cleared by the next begin/reserve).
 * begin/reserve return success; draw returns whether a draw was issued.
 *
 * @example
 * const sprites = new SpriteRenderer(gl);
 * if (!sprites.reserve(1024)) console.warn(sprites.lastError);
 * if (sprites.begin(wasm.exports)) sprites.draw(0);
 *
 * Call begin after the Rust frame is finalized, then draw its batches without
 * mutating them. Wasm memory growth between calls is supported. Payload floats
 * are validated by Rust; this bridge validates counts, byte ranges and batches.
 */
export class SpriteRenderer {
  constructor(gl) {
    this.gl = gl;
    this.program = this.instanceBuffer = this.geometry = this.vao = null;
    this.capacity = this.drawCalls = this.vertices = this.uploadBytes = 0;
    this.lastError = '';
    this.exports = null;
    this._bytes = this._meta = null;
    this._uploadedCount = this._batchCount = this._batchPointer = 0;
  }

  initialize() {
    if (this.program) return;
    const gl = this.gl;
    const shader = (type, source) => {
      const s = gl.createShader(type);
      if (!s) throw new Error('Sprite shader allocation failed');
      gl.shaderSource(s, source);
      gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
        const error = gl.getShaderInfoLog(s);
        gl.deleteShader(s);
        throw new Error(error);
      }
      return s;
    };
    let vertex = null, fragment = null, program = null, geometry = null, instances = null, vao = null;
    try {
      vertex = shader(gl.VERTEX_SHADER, '#version 300 es\nprecision highp float;\n' + spriteVertexBody.replaceAll('attribute ', 'in ').replaceAll('varying ', 'out '));
      fragment = shader(gl.FRAGMENT_SHADER, '#version 300 es\nprecision highp float;\nout vec4 ferravex_pixel;\n' + spriteFragmentBody.replaceAll('varying ', 'in ').replaceAll('gl_FragColor', 'ferravex_pixel'));
      program = gl.createProgram();
      if (!program) throw new Error('Sprite program allocation failed');
      gl.attachShader(program, vertex);
      gl.attachShader(program, fragment);
      ['a_corner', 'a_center', 'a_right', 'a_up', 'a_tint'].forEach((name, i) => gl.bindAttribLocation(program, i, name));
      gl.linkProgram(program);
      if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
      geometry = gl.createBuffer();
      instances = gl.createBuffer();
      vao = gl.createVertexArray();
      if (!geometry || !instances || !vao) throw new Error('Sprite buffer allocation failed');
      gl.bindVertexArray(vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, geometry);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, -1, 1, 1, -1, 1, -1, -1, 1, 1, 1, 0, 1, -1, -1, 1, -1]), gl.STATIC_DRAW);
      if (gl.getBufferParameter(gl.ARRAY_BUFFER, gl.BUFFER_SIZE) !== 72) throw new Error('Sprite geometry allocation failed');
      gl.enableVertexAttribArray(0);
      gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 8, 0);
      gl.bindVertexArray(null);
      this.program = program;
      this.geometry = geometry;
      this.instanceBuffer = instances;
      this.vao = vao;
      this.uploadBytes += 72;
    } catch (error) {
      gl.bindVertexArray(null);
      if (program) gl.deleteProgram(program);
      if (geometry) gl.deleteBuffer(geometry);
      if (instances) gl.deleteBuffer(instances);
      if (vao) gl.deleteVertexArray(vao);
      throw error;
    } finally {
      if (vertex) gl.deleteShader(vertex);
      if (fragment) gl.deleteShader(fragment);
    }
  }

  #reject(error) {
    this.lastError = String(error?.message ?? error).slice(0, 512);
    this._uploadedCount = this._batchCount = 0;
    return false;
  }

  #views(buffer) {
    if (this._bytes?.buffer === buffer) return;
    // Full-memory views let the WebGL2 source-offset/length overload upload an
    // exact range without new views when counts or Rust Vec pointers change.
    this._meta = new DataView(buffer);
    this._bytes = new Uint8Array(buffer);
  }

  /**
   * Reserve space for 0..MAX_SPRITE_INSTANCES, retaining the high-water mark.
   * Zero is a no-op. Growth clears any previously uploaded frame; call begin
   * before drawing again. Failed reservations invalidate that frame as well.
   */
  reserve(count) {
    this.lastError = '';
    if (!validCount(count)) return this.#reject('Sprite capacity must be an integer from 0 to 65536');
    const bytes = count * INSTANCE_BYTES;
    if (bytes <= this.capacity) return true;
    this._uploadedCount = this._batchCount = 0;
    try {
      if (!this.program) this.initialize();
      const gl = this.gl, capacity = Math.min(MAX_INSTANCE_BYTES, Math.max(bytes, this.capacity * 2, 4096));
      gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, capacity, gl.DYNAMIC_DRAW);
      // WebGL reports allocation failure through GL state, not JS exceptions.
      // Query only on growth, never on warmed frames, and do not consume the
      // shared context's error queue with getError().
      const allocated = gl.getBufferParameter(gl.ARRAY_BUFFER, gl.BUFFER_SIZE);
      this.capacity = Number.isSafeInteger(allocated) && allocated >= 0 && allocated <= MAX_INSTANCE_BYTES ? allocated : 0;
      if (this.capacity !== capacity) return this.#reject('Sprite instance buffer allocation failed');
      return true;
    } catch (error) {
      return this.#reject(error);
    }
  }

  begin(exports) {
    this.exports = exports;
    this.uploadBytes = this.vertices = this.drawCalls = 0;
    this._uploadedCount = this._batchCount = 0;
    this.lastError = '';
    try {
      const count = exports.ferravex_sprite_instance_count === undefined ? 0 : exports.ferravex_sprite_instance_count();
      if (!validCount(count)) return this.#reject('Invalid sprite instance count (limit 65536)');
      if (!count) return true;
      const at = exports.ferravex_sprite_instances();
      const batches = exports.ferravex_sprite_batch_count === undefined ? 0 : exports.ferravex_sprite_batch_count();
      if (!validCount(batches)) return this.#reject('Invalid sprite batch count (limit 65536)');
      const batchAt = batches ? exports.ferravex_sprite_batches() : 0;
      // Read memory after all export calls, since a call can grow Wasm memory.
      const buffer = exports.memory.buffer, bytes = count * INSTANCE_BYTES;
      if (!validRange(buffer, at, bytes)) return this.#reject('Invalid sprite instance upload range');
      if (batches && !validRange(buffer, batchAt, batches * BATCH_BYTES)) return this.#reject('Invalid sprite batch range');
      this.#views(buffer);
      if (!this.reserve(count)) return false;
      const gl = this.gl;
      gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceBuffer);
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, this._bytes, at, bytes);
      this.uploadBytes += bytes;
      this._uploadedCount = count;
      this._batchCount = batches;
      this._batchPointer = batchAt;
      return true;
    } catch (error) {
      return this.#reject(error);
    }
  }

  draw(index) {
    if (!this._uploadedCount) return false;
    if (!Number.isInteger(index) || index < 0 || index >= this._batchCount) {
      this.lastError = 'Invalid sprite batch index';
      return false;
    }
    let first, count, flags, triangles;
    try {
      const buffer = this.exports.memory.buffer, at = this._batchPointer + index * BATCH_BYTES;
      if (!validRange(buffer, at, BATCH_BYTES)) return this.#reject('Invalid sprite batch range');
      this.#views(buffer);
      const v = this._meta;
      first = v.getUint32(at, true);
      count = v.getUint32(at + 4, true);
      flags = v.getUint32(at + 8, true);
      triangles = v.getUint32(at + 12, true);
    } catch (error) {
      return this.#reject(error);
    }
    if (!count) return false;
    if (triangles < 1 || triangles > 2 || flags > 7 || first + count > this._uploadedCount) {
      this.lastError = 'Invalid sprite batch data';
      return false;
    }
    const gl = this.gl;
    gl.useProgram(this.program);
    gl.bindVertexArray(this.vao);
    (flags & 1) ? gl.enable(gl.DEPTH_TEST) : gl.disable(gl.DEPTH_TEST);
    gl.depthMask((flags & 1) !== 0 && (flags & 2) === 0);
    gl.enable(gl.BLEND);
    gl.blendFuncSeparate(gl.SRC_ALPHA, (flags & 4) ? gl.ONE : gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceBuffer);
    for (let i = 0; i < 4; i++) {
      gl.enableVertexAttribArray(i + 1);
      gl.vertexAttribPointer(i + 1, 4, gl.FLOAT, false, INSTANCE_BYTES, first * INSTANCE_BYTES + i * 16);
      gl.vertexAttribDivisor(i + 1, 1);
    }
    gl.drawArraysInstanced(gl.TRIANGLES, triangles === 1 ? 6 : 0, triangles * 3, count);
    gl.bindVertexArray(null);
    this.drawCalls++;
    this.vertices += triangles * 3 * count;
    return true;
  }
}
