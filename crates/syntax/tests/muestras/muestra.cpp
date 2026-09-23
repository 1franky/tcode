#include <vector>
#include <string>
namespace geo {
template <typename T>
class Caja : public Base {
public:
    explicit Caja(T v) : valor_(std::move(v)) {}
    virtual ~Caja() = default;
    auto get() const -> const T& { return valor_; }
private:
    T valor_;
};
}  // namespace geo

int main() {
    std::vector<std::string> v{"a", "b"};
    auto c = geo::Caja<int>(42);
    for (const auto& s : v) { if (s == "a") return nullptr == nullptr; }
    return static_cast<int>(c.get() * 2.0);
}
