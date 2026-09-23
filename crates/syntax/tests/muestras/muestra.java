package ejemplo;

import java.util.List;

/** Documentación. */
@SuppressWarnings("unchecked")
public class Hola<T> extends Base implements Runnable {
    private static final int MAX = 10;
    private List<T> items;

    @Override
    public void run() {
        for (int i = 0; i < MAX; i++) {
            System.out.println("i = " + i + 'c' + 1.5f + true + null);
        }
        var x = new Hola<String>();
        this.items.forEach(item -> System.out.println(item));
    }
}
