// SPDX-License-Identifier: GPL-3.0-or-later
import java.util.Locale;

/** Regenerate strict_power_tables.in with Temurin 8. */
public final class StrictPowerTables {
  public static void main(String[] args) {
    System.out.println("(");
    table(4096, i -> StrictMath.log(1.0 + i / 4096.0));
    table(2049, i -> StrictMath.exp(i / 1024.0 - 1.0));
    table(1455, i -> StrictMath.exp(i - 745 + (i < 40 ? 54.0 * StrictMath.log(2.0) : 0.0)));
    System.out.println(")");
  }

  private static void table(int size, java.util.function.IntToDoubleFunction value) {
    System.out.println("  [");
    for (int i = 0; i < size; i++) {
      if (i % 4 == 0) System.out.print("    ");
      System.out.printf(Locale.ROOT, "0x%016x,", Double.doubleToRawLongBits(value.applyAsDouble(i)));
      System.out.print(i % 4 == 3 || i == size - 1 ? "\n" : " ");
    }
    System.out.println("  ],");
  }
}
