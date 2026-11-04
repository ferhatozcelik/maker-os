// Package apierr, API hata gövdesini ({"error":{"code","message"}}) taşıyan
// hata türünü tanımlar. Modüller bu türü döndürür; httpd onu HTTP durumuna çevirir.
package apierr

import (
	"errors"
	"net/http"
)

// Error, istemciye gösterilebilen bir API hatasıdır. Message Türkçedir.
type Error struct {
	Status  int
	Code    string
	Message string
}

func (e *Error) Error() string { return e.Code + ": " + e.Message }

// New yeni bir API hatası üretir.
func New(status int, code, msg string) *Error {
	return &Error{Status: status, Code: code, Message: msg}
}

func Invalid(msg string) *Error      { return New(http.StatusBadRequest, "invalid", msg) }
func Unauthorized(msg string) *Error { return New(http.StatusUnauthorized, "unauthorized", msg) }
func Forbidden(msg string) *Error    { return New(http.StatusForbidden, "forbidden", msg) }
func NotFound(msg string) *Error     { return New(http.StatusNotFound, "not_found", msg) }
func Conflict(msg string) *Error     { return New(http.StatusConflict, "conflict", msg) }
func Internal(msg string) *Error     { return New(http.StatusInternalServerError, "internal", msg) }
func Unavailable(msg string) *Error  { return New(http.StatusServiceUnavailable, "unavailable", msg) }

// As, err zincirinde bir *Error varsa onu döndürür.
func As(err error) (*Error, bool) {
	var e *Error
	if errors.As(err, &e) {
		return e, true
	}
	return nil, false
}
