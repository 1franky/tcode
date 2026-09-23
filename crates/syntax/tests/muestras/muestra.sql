-- comentario
CREATE TABLE usuarios (
    id INTEGER PRIMARY KEY,
    nombre VARCHAR(100) NOT NULL DEFAULT 'anon',
    creado TIMESTAMP
);

SELECT u.id, COUNT(*) AS total, MAX(p.monto) * 1.5
FROM usuarios u
LEFT JOIN pedidos p ON p.usuario_id = u.id
WHERE u.nombre LIKE 'a%' AND p.monto > 100
GROUP BY u.id
HAVING COUNT(*) > 2
ORDER BY total DESC;

INSERT INTO usuarios (id, nombre) VALUES (1, 'Ana');
UPDATE usuarios SET nombre = NULL WHERE id = 1;
