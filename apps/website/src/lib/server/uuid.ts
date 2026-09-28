import { validate as validateUuid, version as uuidVersion } from 'uuid';

export function isUuidV7(value: string) {
	return validateUuid(value) && uuidVersion(value) === 7;
}
