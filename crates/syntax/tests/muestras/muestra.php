<?php
namespace App\Modelos;

use App\Base;

// comentario
final class Usuario extends Base implements \JsonSerializable
{
    public const MAX = 10;
    private ?string $nombre = null;

    public function __construct(private int $id, string $nombre = "anon")
    {
        $this->nombre = $nombre;
    }

    public static function crear(array $datos): static
    {
        foreach ($datos as $k => $v) { echo "k: {$k} " . strlen($v) . PHP_EOL; }
        return new static(1, 'x');
    }

    public function jsonSerialize(): mixed { return ['id' => $this->id, 'ok' => true]; }
}
