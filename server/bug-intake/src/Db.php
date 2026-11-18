<?php
declare(strict_types=1);

namespace NebulaeIntake;

use PDO;

/** Thin PDO wrapper. Timestamps are generated in PHP (UTC) so MySQL and SQLite behave identically. */
final class Db
{
    public function __construct(public readonly PDO $pdo)
    {
        $this->pdo->setAttribute(PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION);
        $this->pdo->setAttribute(PDO::ATTR_DEFAULT_FETCH_MODE, PDO::FETCH_ASSOC);
    }

    public static function connect(Config $c): self
    {
        $pdo = new PDO($c->str('DB_DSN'), $c->str('DB_USER') ?: null, $c->str('DB_PASS') ?: null);
        return new self($pdo);
    }

    public static function memory(): self
    {
        return new self(new PDO('sqlite::memory:'));
    }

    public function driver(): string
    {
        return (string) $this->pdo->getAttribute(PDO::ATTR_DRIVER_NAME);
    }

    public static function now(?int $offsetSeconds = null): string
    {
        return gmdate('Y-m-d H:i:s', time() + ($offsetSeconds ?? 0));
    }

    /** @param array<int|string,mixed> $params */
    public function run(string $sql, array $params = []): \PDOStatement
    {
        $st = $this->pdo->prepare($sql);
        $st->execute($params);
        return $st;
    }

    /** @param array<int|string,mixed> $params @return array<string,mixed>|null */
    public function one(string $sql, array $params = []): ?array
    {
        $row = $this->run($sql, $params)->fetch();
        return $row === false ? null : $row;
    }

    /** @param array<int|string,mixed> $params @return list<array<string,mixed>> */
    public function all(string $sql, array $params = []): array
    {
        return $this->run($sql, $params)->fetchAll();
    }

    /** @template T @param callable():T $fn @return T */
    public function transaction(callable $fn)
    {
        $this->pdo->beginTransaction();
        try {
            $r = $fn();
            $this->pdo->commit();
            return $r;
        } catch (\Throwable $e) {
            if ($this->pdo->inTransaction()) {
                $this->pdo->rollBack();
            }
            throw $e;
        }
    }
}
