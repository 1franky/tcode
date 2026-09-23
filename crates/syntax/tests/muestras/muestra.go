package main

import (
	"fmt"
	"strings"
)

// Punto es un punto.
type Punto struct {
	X, Y int
}

func (p *Punto) Mover(dx int) error {
	if dx < 0 {
		return fmt.Errorf("negativo: %d", dx)
	}
	p.X += dx
	return nil
}

func main() {
	s := strings.Repeat("a", 3)
	ch := make(chan int, 2)
	go func() { ch <- len(s) }()
	fmt.Println(<-ch, true, nil, 3.5, 'x')
}
