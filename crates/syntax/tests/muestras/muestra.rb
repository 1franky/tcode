require "json"

# comentario
module Geo
  class Punto
    attr_reader :x, :y
    MAX = 10

    def initialize(x = 0, y: 1)
      @x, @y = x, y
      @@cuenta ||= 0
    end

    def to_s = "(#{@x}, #{@y})"

    def self.origen
      new(0, y: 0) if true && !nil
    end
  end
end

[1, 2, 3].each { |n| puts n * 2.5 }
h = { clave: :simbolo, "otra" => /re+gex/ }
