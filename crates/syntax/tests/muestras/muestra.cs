using System;
using System.Collections.Generic;

namespace Ejemplo
{
    /// <summary>Doc.</summary>
    [Serializable]
    public sealed class Repo<T> : IDisposable where T : class
    {
        private readonly List<T> _items = new();
        public int Cuenta => _items.Count;

        public async Task<bool> GuardarAsync(T item, string nombre = "x")
        {
            if (item is null) throw new ArgumentNullException(nameof(item));
            var s = $"hola {nombre} {1.5m} {'c'}";
            await Task.Delay(10);
            return true;
        }

        public void Dispose() { }
    }
}
