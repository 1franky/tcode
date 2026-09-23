// comentario
import { leer } from "./io.js";
const PI = 3.14;
export class Figura extends Base {
  constructor(nombre) { super(); this.nombre = nombre; }
  async area(r = 2) { return PI * r ** 2; }
}
function saludar(nombre) {
  const msg = `hola ${nombre}`;
  let x = /ab+c/g.test(msg) ? 1 : null;
  console.log(msg, x, undefined, true);
  return [1, 2, 3].map((n) => n * 2);
}
