
#include <string>
#include <print>

#include "encode.h"
#include "decode.h"

int main()
{
#if 1
	auto field = librscpp::GF2n<>(0b1'0001'1101, 0); // QR Code: x^8 + x^4 + x^3 + x^2 + 1
#else
	auto field = librscpp::GFp<>(251, 6, 1);
#endif
	std::println("Using field: GF({}) and fcr: {}", field.size(), field.fcr());

	const std::string data = "Hello, World!";
	constexpr int paritySize = 4;

	auto codeword = librscpp::encode(field, data, paritySize);
	std::println("Encoded codeword: '{}'", codeword);

	// Simulate a couple of symbol errors.
	codeword[3] = codeword[8] = '*';
	std::println("Corrupt codeword: '{}'", codeword);

	auto res = librscpp::decode(field, codeword, paritySize);
	if (res)
		std::println("Decoded codeword: '{}'\nUsed parity symbols: {}", codeword, *res);
	else
		std::println("Failed to decode codeword.");

	return 0;
}