import { defineCollection } from "astro:content";
import { glob } from "astro/loaders";
import { z } from "astro/zod";

const MINIMUM_TEXT_LENGTH = 1;

const adrs = defineCollection({
  loader: glob({ pattern: "*.md", base: "../docs/adr" }),
  schema: z.object({
    number: z.number().int().positive(),
    title: z.string().min(MINIMUM_TEXT_LENGTH),
    status: z.string().min(MINIMUM_TEXT_LENGTH),
    date: z.coerce.date().optional(),
  }),
});

export const collections = { adrs };
