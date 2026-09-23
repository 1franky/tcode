package ejemplo

import kotlin.math.sqrt

// comentario
data class Punto(val x: Double, val y: Double = 0.0) {
    fun norma(): Double = sqrt(x * x + y * y)
}

sealed interface Forma
object Vacia : Forma

fun main(args: Array<String>) {
    val p = Punto(3.0, 4.0)
    var n: Int? = null
    when (p.norma()) {
        5.0 -> println("cinco ${p.x} $n")
        else -> println('x')
    }
    listOf(1, 2, 3).filter { it > 1 }.forEach(::println)
}
