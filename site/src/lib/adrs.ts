import { getCollection, type CollectionEntry } from "astro:content";
import { validateAdrReferences } from "./adr-reference";
export { requireAdr } from "./adr-reference";

export interface AdrRecord {
  entry: CollectionEntry<"adrs">;
  label: string;
  slug: string;
}

const ADR_NUMBER_WIDTH = 4;
const paddedNumber = (number: number) =>
  number.toString().padStart(ADR_NUMBER_WIDTH, "0");

export async function getAdrs(): Promise<AdrRecord[]> {
  const entries = await getCollection("adrs");
  const records = entries.map((entry) => {
    const slug = entry.id.replace(/\.md$/, "");
    const prefix = paddedNumber(entry.data.number);
    if (!slug.startsWith(`${prefix}-`)) {
      throw new Error(
        `ADR ${prefix} must use a canonical filename beginning with ${prefix}-`,
      );
    }
    return { entry, label: `ADR ${prefix}`, slug };
  });

  records.sort(
    (left, right) => left.entry.data.number - right.entry.data.number,
  );
  const numbers = records.map(({ entry }) => entry.data.number);
  if (new Set(numbers).size !== numbers.length) {
    throw new Error("ADR numbers must be unique");
  }
  validateAdrReferences(records);
  return records;
}

export const adrPath = (record: AdrRecord) =>
  `docs/architecture/decisions/${record.slug}/`;
