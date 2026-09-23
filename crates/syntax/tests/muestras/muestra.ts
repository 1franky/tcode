interface Punto { x: number; y?: string }
type Id = string | number;
enum Color { Rojo, Verde }
export abstract class Repo<T extends Punto> implements Iterable<T> {
  private items: T[] = [];
  public static crear(): Repo<Punto> { return null as any; }
  *[Symbol.iterator]() { yield* this.items; }
}
const f = async (a: Id): Promise<void> => { await fetch(`/api/${a}`); };
const el = <div className="x">{f}</div>;
