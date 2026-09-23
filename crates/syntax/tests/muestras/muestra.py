import os
from dataclasses import dataclass

CONSTANTE = 42

@dataclass
class Punto:
    """Un punto en el plano."""
    x: float = 0.0
    y: float = 0.0

    def distancia(self, otro: "Punto") -> float:
        # comentario
        return ((self.x - otro.x) ** 2 + (self.y - otro.y) ** 2) ** 0.5

def main(args=None):
    ruta = os.path.join("a", f"b{CONSTANTE}")
    for i in range(10):
        if i % 2 == 0 and not None:
            print(i, ruta, True, [1, 2.5, 'x'], {"k": lambda v: v})
    return len(args or [])
