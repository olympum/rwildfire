// SPDX-License-Identifier: GPL-3.0-or-later
// Regenerate crates/cpu/src/strict_trig_table.in with JDK 8.
public final class StrictTrigTable {
  public static void main(String[] args) {
    int size = 2048;
    double stepHi = 4.0 * Double.longBitsToDouble(4609753056924401664L) / size;
    double stepLo = 4.0 * Double.longBitsToDouble(4454258360616903473L) / size;
    System.out.println("[");
    for (int i = 0; i < size; i++) {
      double angle = i * stepHi + i * stepLo;
      double sin = i == 0 || i == size / 2 ? 0.0 : StrictMath.sin(angle);
      double cos = i == size / 4 || i == 3 * size / 4 ? 0.0 : StrictMath.cos(angle);
      System.out.printf("    (0x%016x, 0x%016x),%n", Double.doubleToRawLongBits(sin), Double.doubleToRawLongBits(cos));
    }
    System.out.println("]");
  }
}
