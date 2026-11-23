// SPDX-License-Identifier: GPL-3.0-or-later
// Regenerate crates/cpu/src/strict_atan_table.in with JDK 8. See NOTICE.md.
public final class StrictAtanTable {
  public static void main(String[] args) {
    int intervals = 4096;
    double step = StrictMath.tan(Math.toRadians(74.0)) / intervals;
    System.out.println("[");
    for (int i = 0; i <= intervals; i++) {
      System.out.printf("    0x%016x,%n", Double.doubleToRawLongBits(StrictMath.atan(i * step)));
    }
    System.out.println("]");
    System.err.printf("step bits: 0x%016x%n", Double.doubleToRawLongBits(step));
  }
}
