const ADR_NUMBER_WIDTH = 4;
const paddedNumber = (number: number) =>
  number.toString().padStart(ADR_NUMBER_WIDTH, "0");

interface NumberedAdr {
  entry: { data: { number: number } };
}

export const DECISION_PROCESS_ADR = 1;
export const FUNCTIONAL_CORE_ADR = 2;
export const SIGNING_FAILOVER_ADR = 3;
export const HOME_MANAGER_ADR = 5;
export const STAGED_RELEASE_ADR = 6;

const REFERENCED_ADRS = [
  DECISION_PROCESS_ADR,
  FUNCTIONAL_CORE_ADR,
  SIGNING_FAILOVER_ADR,
  HOME_MANAGER_ADR,
  STAGED_RELEASE_ADR,
];

export function requireAdr<Record extends NumberedAdr>(
  records: Record[],
  number: number,
): Record {
  const record = records.find(
    (candidate) => candidate.entry.data.number === number,
  );
  if (!record) {
    throw new Error(`Referenced ADR ${paddedNumber(number)} is missing`);
  }
  return record;
}

export function validateAdrReferences(records: NumberedAdr[]): void {
  for (const number of REFERENCED_ADRS) {
    requireAdr(records, number);
  }
}
